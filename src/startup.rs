use crate::configuration::Settings;
use crate::helpers;
use actix_cors::Cors;
use actix_web::dev::Server;
use actix_web::{
    web::{self},
    App, HttpServer,
};
use crate::middleware;
use actix_web_httpauth::middleware::HttpAuthentication;
use sqlx::{Pool, Postgres};
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub async fn run(
    listener: TcpListener,
    pg_pool: Pool<Postgres>,
    settings: Settings,
) -> Result<Server, std::io::Error> {
    let settings = web::Data::new(settings);
    let pg_pool = web::Data::new(pg_pool);

    let mq_manager = helpers::MqManager::try_new(settings.amqp.connection_string())?;
    let mq_manager = web::Data::new(mq_manager);

    let access_control_manager = middleware::access_manager::try_new().await?;

    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .service(web::scope("/health_check").service(crate::routes::health_check))
            .service(
                web::scope("/client")
                    .wrap(HttpAuthentication::bearer(
                        middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::client::add_handler)
                    .service(crate::routes::client::update_handler)
                    .service(crate::routes::client::enable_handler)
                    .service(crate::routes::client::disable_handler),
            )
            .service(
                web::scope("/test")
                    .wrap(middleware::client::Guard::new())
                    .wrap(Cors::permissive())
                    .service(crate::routes::test::deploy::handler),
            )
            .service(
                web::scope("/pen/1")
                    .wrap(access_control_manager.clone()) 
                    .wrap(HttpAuthentication::bearer(
                        middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::test::casbin::handler),
            )
            .service(
                web::scope("/rating")
                    .wrap(HttpAuthentication::bearer(
                        middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::rating::add_handler)
                    .service(crate::routes::rating::get_handler)
                    .service(crate::routes::rating::list_handler),
            )
            .service(
                web::scope("/stack")
                    .wrap(HttpAuthentication::bearer(
                        middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::stack::deploy::add)
                    .service(crate::routes::stack::compose::add)
                    .service(crate::routes::stack::compose::admin)
                    .service(crate::routes::stack::get::item)
                    .service(crate::routes::stack::get::list)
                    .service(crate::routes::stack::add::add)
                    .service(crate::routes::stack::update::update),
            )
            .app_data(pg_pool.clone())
            .app_data(mq_manager.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
