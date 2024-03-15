use crate::configuration::Settings;
use crate::helpers;
use crate::routes;
use actix_cors::Cors;
use actix_web::{
    dev::Server,
    http,
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
    let json_config = web::JsonConfig::default()
        .error_handler(|err, req| {
            let msg: String = match err {
                 error::JsonPayloadError::Deserialize(err) => format!("{{\"kind\":\"deserialize\",\"line\":{}, \"column\":{}, \"msg\":\"{}\"}}", err.line(), err.column(), err),
                 _ => format!("{{\"kind\":\"other\",\"msg\":\"{}\"}}", err)
            };
            error::InternalError::new(msg, http::StatusCode::BAD_REQUEST).into()
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
                web::scope("/test")
                    .service(routes::test::deploy::handler)
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
            .app_data(json_config.clone())
            .app_data(pg_pool.clone())
            .app_data(mq_manager.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}

fn line_column_to_index(u8slice: &[u8], line: usize, column: usize) -> usize {
    let mut l = 1;
    let mut c = 0;
    let mut i = 0;
    for ch in u8slice {
        i += 1;
        match ch {
            b'\n' => {
                l += 1;
                c = 0;
            }
            _ => {
                c += 1;
            }
        }

        if line == l && c == column {
            break;
        }
    }

    return i;
}
