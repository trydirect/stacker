//! Credentials never reach the logs. Every auth method runs in a tracing
//! span; a span that records the whole request writes its headers and query
//! string (bearer, cookie and agent tokens) into the JSON log on every request.

use super::*;
use crate::configuration::Settings;
use actix_web::{test, web, App, HttpResponse};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::layer::SubscriberExt;

const SECRET: &str = "s3cr3t-credential-value";

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[actix_web::test]
async fn no_credential_reaches_the_auth_spans() {
    let logs = Captured::default();
    let writer = logs.clone();
    // The production log format (src/telemetry.rs), every level.
    let subscriber = tracing_subscriber::Registry::default()
        .with(JsonStorageLayer)
        .with(BunyanFormattingLayer::new("test".into(), move || {
            writer.clone()
        }));
    let _guard = tracing::subscriber::set_default(subscriber);

    let app = test::init_service(
        App::new()
            .wrap(Manager::new())
            .app_data(web::Data::new(Settings {
                // Nothing listens there: the request to the User Service fails fast.
                auth_url: "http://127.0.0.1:9/me".to_string(),
                ..Settings::default()
            }))
            .app_data(web::Data::new(reqwest::Client::new()))
            .app_data(web::Data::new(OAuthCache::new(Duration::from_secs(60))))
            .default_service(web::to(HttpResponse::Ok)),
    )
    .await;

    for req in [
        test::TestRequest::get()
            .uri("/api/v1/project")
            .insert_header(("authorization", format!("Bearer {SECRET}"))),
        test::TestRequest::get()
            .uri("/api/v1/project")
            .insert_header(("cookie", format!("access_token={SECRET}"))),
        test::TestRequest::get().uri(&format!("/mcp?access_token={SECRET}")),
    ] {
        let _ = test::try_call_service(&app, req.to_request()).await;
    }

    let logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("AUTHENTICATE WITH BEARER TOKEN - START"),
        "the auth spans must be captured for this test to mean anything:\n{logs}"
    );
    assert!(
        !logs.contains(SECRET),
        "a credential reached the log:\n{logs}"
    );
}
