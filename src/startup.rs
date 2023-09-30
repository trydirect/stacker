use actix_cors::Cors;
use actix_web::dev::{Server, ServiceRequest};
use actix_web::middleware::Logger;
use actix_web::HttpMessage;
use actix_web::{
    // http::header::HeaderName,
    web::{self},
    App,
    Error,
    HttpServer,
};
use actix_web_httpauth::{extractors::bearer::BearerAuth, middleware::HttpAuthentication};
use sqlx::PgPool;
use std::net::TcpListener;

use crate::models::user::User;

async fn bearer_guard(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    eprintln!("{credentials:?}");
    //todo check that credentials.token is a real. get in sync with auth server
    //todo get user from auth server
    //todo save the server in the request state
    //todo get the user in the rating route
    let user = User { id: 1 };
    tracing::info!("authentication middleware. {user:?}");
    let existent_user = req.extensions_mut().insert(user);
    if existent_user.is_some() {
        tracing::error!("authentication middleware. already logged {existent_user:?}");
        //return Err(("".into(), req));
    }
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
    })
    .listen(listener)?
    .run();

    Ok(server)
}
