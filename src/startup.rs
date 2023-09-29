use actix_cors::Cors;
use actix_web::dev::{Server, ServiceRequest};
use actix_web::middleware::Logger;
use actix_web::{
    // http::header::HeaderName,
    web::{self, Form},
    App,
    Error,
    HttpServer,
};
use actix_web_httpauth::{extractors::bearer::BearerAuth, middleware::HttpAuthentication};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::TcpListener;

#[derive(Serialize, Deserialize, Debug)]
pub struct AppState {
    pub user_id: i32, // @todo User must be move later to actix session and obtained from auth
}

async fn bearer_guard(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    eprintln!("{credentials:?}");
    //todo check that credentials.token is a real. get in sync with auth server
    //todo get user from auth server
    //todo save the server in the request state
    //todo get the user in the rating route
    Ok(req)
}

pub fn run(listener: TcpListener, db_pool: PgPool) -> Result<Server, std::io::Error> {
    let db_pool = web::Data::new(db_pool);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
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
            .app_data(web::Data::new(AppState { user_id: 1 }))
    })
    .listen(listener)?
    .run();

    Ok(server)
}
