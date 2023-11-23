use crate::configuration::Settings;
use actix_cors::Cors;
use actix_web::dev::Server;
use actix_web::{
    web::{self},
    App, HttpServer,
};
use actix_web_httpauth::middleware::HttpAuthentication;
use sqlx::{Pool, Postgres};
use std::net::TcpListener;
use std::sync::Arc;
use tracing_actix_web::TracingLogger;

pub async fn run(
    listener: TcpListener,
    db_pool: Pool<Postgres>,
    settings: Settings,
) -> Result<Server, std::io::Error> {
    let settings = web::Data::new(Arc::new(settings));
    let db_pool = web::Data::new(db_pool);

    // let address = format!("{}:{}", settings.app_host, settings.app_port);
    // tracing::info!("Start server at {:?}", &address);
    // let listener = std::net::TcpListener::bind(address)
    //     .expect(&format!("failed to bind to {}", settings.app_port));

    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .service(web::scope("/health_check")
                .service(crate::routes::health_check))
            .service(
                web::scope("/client")
                    .wrap(HttpAuthentication::bearer(
                        crate::middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::client::add_handler)
                    .service(crate::routes::client::update_handler)
                    .service(crate::routes::client::enable_handler)
                    .service(crate::routes::client::disable_handler),
            )
            .service(
                web::scope("/test")
                    .wrap(crate::middleware::client::Guard::new())
                    .wrap(Cors::permissive())
                    .service(crate::routes::test::deploy::handler),
            )
            .service(
                web::scope("/rating")
                    .wrap(HttpAuthentication::bearer(
                        crate::middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::rating::add_handler)
                    .service(crate::routes::rating::get_handler)
                    .service(crate::routes::rating::list_handler),
            )
            .service(
                web::scope("/stack")
                    .wrap(HttpAuthentication::bearer(
                        crate::middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::stack::deploy::add)
                    .service(crate::routes::stack::compose::add)
                    .service(crate::routes::stack::compose::admin)
                    .service(crate::routes::stack::get::item)
                    .service(crate::routes::stack::get::list)
                    .service(crate::routes::stack::add::add),
            )
            .app_data(db_pool.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
