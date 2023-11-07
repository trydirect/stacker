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
            .service(web::scope("/health_check").service(crate::routes::health_check))
            .service(
                web::scope("/client")
                    .wrap(HttpAuthentication::bearer(
                        crate::middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::client::add_handler)
                    .service(crate::routes::client::update_handler)
                    .service(crate::routes::client::disable_handler),
            )
            .service(
                //todo 1. add client_guard. it should fetch client_id and hash from headers. based on db's
                //client secret and input body valiates the input. the client is to be handed over
                //to the http endpoint
                //todo 2. the generation secret and the client bearer to be separated in a separate
                //utils module
                web::scope("/test").service(crate::routes::test::deploy::handler),
            )
            .service(
                web::scope("/rating")
                    .wrap(HttpAuthentication::bearer(
                        crate::middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::rating::add_handler)
                    .service(crate::routes::rating::get_handler)
                    .service(crate::routes::rating::default),
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
            .service(
                web::scope("/stack")
                    .wrap(HttpAuthentication::bearer(
                        crate::middleware::trydirect::bearer_guard,
                    ))
                    .wrap(Cors::permissive())
                    .service(crate::routes::stack::add::add)
                    .service(crate::routes::stack::get::get),
            )
            .app_data(db_pool.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
