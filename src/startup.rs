use crate::configuration::Settings;
use crate::helpers;
use crate::routes;
use actix_cors::Cors;
use actix_web::dev::Server;
use actix_web::{
    web::{self},
    App, HttpServer,
};
use crate::middleware;
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

    let authorization = middleware::authorization::try_new(settings.database.connection_string()).await?;

    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .wrap(authorization.clone()) 
            .wrap(middleware::authentication::Manager::new())
            .wrap(Cors::permissive())
            .service(
                web::scope("/health_check").service(routes::health_check)
            )
            .service(
                web::scope("/client")
                    .service(routes::client::add_handler)
                    .service(routes::client::update_handler)
                    .service(routes::client::enable_handler)
                    .service(routes::client::disable_handler),
            )
            .service(
                web::scope("/test").service(routes::test::deploy::handler),
            )
            .service(
                web::scope("/pen/1").service(routes::test::casbin::handler),
            )
            .service(
                web::scope("/rating")
                    .service(routes::rating::add_handler)
                    .service(routes::rating::get_handler)
                    .service(routes::rating::list_handler),
            )
            .service(
                web::scope("/stack")
                    .service(routes::stack::deploy::add)
                    .service(routes::stack::compose::add)
                    .service(routes::stack::compose::admin)
                    .service(routes::stack::get::item)
                    .service(routes::stack::get::list)
                    .service(routes::stack::add::add)
                    .service(routes::stack::update::update),
            )
            .app_data(pg_pool.clone())
            .app_data(mq_manager.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
