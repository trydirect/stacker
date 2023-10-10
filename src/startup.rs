use crate::configuration::Settings;
use actix_cors::Cors;
use actix_web::dev::{Server, ServiceRequest};
use actix_web::error::{ErrorInternalServerError, ErrorUnauthorized};
use actix_web::{
    // http::header::HeaderName,
    web::{self},
    App,
    Error,
    HttpServer,
};
use actix_web::{HttpMessage, ResponseError};
use actix_web_httpauth::{extractors::bearer::BearerAuth, middleware::HttpAuthentication};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use sqlx::PgPool;
use std::net::TcpListener;
use std::sync::Arc;
use tracing_actix_web::TracingLogger;

use crate::models::user::User;

#[tracing::instrument(name = "Bearer guard.")]
async fn bearer_guard(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let settings = req.app_data::<Arc<Settings>>().unwrap();

    let client = reqwest::Client::new();
    let resp = client
        .get(&settings.auth_url)
        .bearer_auth(credentials.token())
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .send()
        .await;

    let resp = match resp {
        Ok(resp) if resp.status().is_success() => resp,
        Ok(resp) => {
            tracing::error!("Authentication service returned no success {:?}", resp);
            return Err((ErrorUnauthorized(""), req));
        }
        Err(err) => {
            tracing::error!("error from reqwest {:?}", err);
            return Err((ErrorInternalServerError(""), req));
        }
    };

    let user: User = match resp.json().await {
        Ok(user) => {
            tracing::info!("unpacked user {user:?}");
            user
        }
        Err(err) => {
            tracing::error!("can't parse the response body {:?}", err);
            return Err((ErrorUnauthorized(""), req));
        }
    };

    let existent_user = req.extensions_mut().insert(user);
    if existent_user.is_some() {
        tracing::error!("already logged {existent_user:?}");
        return Err((ErrorInternalServerError(""), req));
    }

    Ok(req)
}

pub async fn run(settings: Settings) -> Result<Server, std::io::Error> {
    let settings = Arc::new(settings);
    let db_pool = PgPool::connect(&settings.database.connection_string())
        .await
        .expect("Failed to connect to database.");
    let db_pool = web::Data::new(db_pool);

    let address = format!("127.0.0.1:{}", settings.application_port);
    tracing::info!("Start server at {:?}", &address);
    let listener = std::net::TcpListener::bind(address)
        .expect(&format!("failed to bind to {}", settings.application_port));

    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .wrap(HttpAuthentication::bearer(bearer_guard))
            .wrap(Cors::permissive())
            .service(
                web::resource("/health_check").route(web::get().to(crate::routes::health_check)),
            )
            .service(
                web::scope("/rating")
                    .service(crate::routes::add_handler)
                    .service(crate::routes::get_handler),
            )
            // .service(
            //     web::resource("/stack/{id}")
            //         .route(web::get()
            //             .to(crate::routes::stack::get))
            //         .route(web::post()
            //             .to(crate::routes::stack::update))
            //         .route(web::post()
            //             .to(crate::routes::stack::add)),
            // )
            .service(web::resource("/stack").route(web::post().to(crate::routes::stack::add::add)))
            .service(
                web::resource("/stack/deploy").route(web::post().to(crate::routes::stack::deploy)),
            )
            .app_data(db_pool.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
