use actix_cors::Cors;
use actix_web::dev::{Server, ServiceRequest};
use actix_web::error::ErrorUnauthorized;
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
        .get("https://a65190108818c4e98ac6000e4.mockapi.io/user/1") //todo add the right url
        .bearer_auth(credentials.token())
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .send()
        .await;

    let resp = match resp {
        Ok(resp) => resp,
        Err(err) => {
            tracing::error!("{:?}", err);

            return Err((ErrorUnauthorized(err), req));
        }
    };
    return Err((
        ErrorUnauthorized(std::io::Error::new(std::io::ErrorKind::Other, "oh no!")),
        req,
    ));

    let user: User = match resp.status() {
        reqwest::StatusCode::OK => match resp.json().await {
            Ok(user) => user,
            Err(err) => panic!("can't parse the user from json {err:?}"), //todo
        },
        other => {
            //todo process the other status code accordingly
            panic!("unexpected status code {other}");
        }
    };

    //let user = User { id: 1 };
    tracing::info!("unpacked user {user:?}");
    let existent_user = req.extensions_mut().insert(user);
    if existent_user.is_some() {
        tracing::error!("already logged {existent_user:?}");
        //return Err(("".into(), req));
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
