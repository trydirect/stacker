use actix_web::dev::Server;
use actix_web::middleware::Logger;
use actix_web::{
    http::header::HeaderName,
    web::{self, Form},
    App, HttpServer,
};
use sqlx::PgPool;
use std::net::TcpListener;

pub fn run(listener: TcpListener, db_pool: PgPool) -> Result<Server, std::io::Error> {
    let db_pool = web::Data::new(db_pool);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .service(
                web::resource("/health_check")
                    .route(web::get()
                        .to(crate::routes::health_check)),
            )
            .service(
                web::resource("/rating")
                    .route(web::get().to(crate::routes::rating))
                    .route(web::post().to(crate::routes::rating)),
            )
            .app_data(db_pool.clone())
    })
        .listen(listener)?
        .run();

    Ok(server)
}
