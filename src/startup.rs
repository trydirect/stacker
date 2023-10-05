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
use tracing_actix_web::TracingLogger;

use crate::models::user::User;

#[tracing::instrument(name = "Bearer guard.")]
async fn bearer_guard(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://65190108818c4e98ac6000e4.mockapi.io/user/1") //todo add the right url
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

pub fn run(listener: TcpListener, db_pool: PgPool) -> Result<Server, std::io::Error> {
    let db_pool = web::Data::new(db_pool);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .wrap(HttpAuthentication::bearer(bearer_guard))
            .wrap(Cors::permissive())
            .service(
                web::resource("/health_check").route(web::get().to(crate::routes::health_check)),
            )
            .service(
                web::resource("/rating")
                    .route(web::get().to(crate::routes::rating))
                    .route(web::post().to(crate::routes::rating)),
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
    })
    .listen(listener)?
    .run();

    Ok(server)
}
