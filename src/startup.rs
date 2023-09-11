use actix_web::dev::Server;
use actix_web::middleware::Logger;
use actix_web::{
    http::header::HeaderName,
    web::{self, Form},
    App, HttpServer,
};
use sqlx::PgPool;
use std::net::TcpListener;
use uuid::Uuid;

pub struct AppState {
   pub user_id: Uuid // @todo User must be move lates to actix session and obtained from auth
}


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
            .app_data(web::Data::new(AppState {
                user_id: Uuid::new_v4(),
            }))
    })
        .listen(listener)?
        .run();

    Ok(server)
}
