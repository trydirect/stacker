use crate::configuration::Settings;
use crate::helpers;
use crate::routes;
use actix_cors::Cors;
use actix_web::dev::Server;
use actix_web::{
    error,
    web,
    App,
    HttpServer,
    HttpResponse,
    FromRequest,
    rt,
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
    let json_cfg = web::JsonConfig::default()
        .error_handler(|err, req| {
            let bytes = web::Bytes::from_request(req, &mut actix_web::dev::Payload::None);
            let rt = rt::Runtime::new().unwrap();
            let handle = rt.spawn(async move {
                let bytes: web::Bytes = bytes.await.unwrap();
            });

            rt.block_on(handle).unwrap(); //todo 1. get the result of bytes. 
                                          //todo 2. transform line, column into index
                                          //todo 3. transform index in the start of json till the
                                          //error occured

            match err {
                error::JsonPayloadError::Deserialize(ref err) => println!("deserialize {err:?} {err}"),
                _ => println!("sth else {err:?}")
            }

            error::InternalError::from_response(err, HttpResponse::Conflict().into()).into()
        });

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
                web::scope("/admin/client")
                    .service(routes::client::admin_enable_handler)
                    .service(routes::client::admin_update_handler)
                    .service(routes::client::admin_disable_handler),
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
            .service(
                web::scope("/admin/stack")
                    .service(routes::stack::get::admin_item)
                    .service(routes::stack::get::admin_list)
            )
            .app_data(json_cfg.clone())
            .app_data(pg_pool.clone())
            .app_data(mq_manager.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
