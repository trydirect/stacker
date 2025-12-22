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

    let vault_client = helpers::VaultClient::new(&settings.vault);
    let vault_client = web::Data::new(vault_client);

    let authorization = middleware::authorization::try_new(settings.database.connection_string()).await?;
    let json_config = web::JsonConfig::default()
        .error_handler(|err, _req| { //todo
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
                web::scope("/test")
                    .service(routes::test::deploy::handler)
            )
            .service(
                web::scope("/rating")
                    .service(routes::rating::anonymous_get_handler)
                    .service(routes::rating::anonymous_list_handler)
                    .service(routes::rating::user_add_handler)
                    .service(routes::rating::user_delete_handler)
                    .service(routes::rating::user_edit_handler),
            )
            .service(
                web::scope("/project")
                    .service(crate::routes::project::deploy::item)
                    .service(crate::routes::project::deploy::saved_item)
                    .service(crate::routes::project::compose::add)
                    .service(crate::routes::project::get::list)
                    .service(crate::routes::project::get::item)
                    .service(crate::routes::project::add::item)
                    .service(crate::routes::project::update::item) 
                    .service(crate::routes::project::delete::item),
            )
            .service(
                web::scope("/admin")
                    .service(
                        web::scope("/rating")
                            .service(routes::rating::admin_get_handler)
                            .service(routes::rating::admin_list_handler)
                            .service(routes::rating::admin_edit_handler)
                            .service(routes::rating::admin_delete_handler),
                    )
                    .service(
                        web::scope("/project")
                            .service(crate::routes::project::get::admin_list)
                            .service(crate::routes::project::compose::admin),
                    )
                    .service(
                        web::scope("/client")
                            .service(routes::client::admin_enable_handler)
                            .service(routes::client::admin_update_handler)
                            .service(routes::client::admin_disable_handler),
                    )
                    .service(
                        web::scope("/agreement")
                            .service(routes::agreement::admin_add_handler)
                            .service(routes::agreement::admin_update_handler)
                            .service(routes::agreement::get_handler),
                    )
            )
            .service(
                web::scope("/cloud")
                    .service(crate::routes::cloud::get::item)
                    .service(crate::routes::cloud::get::list)
                    .service(crate::routes::cloud::add::add)
                    .service(crate::routes::cloud::update::item)
                    .service(crate::routes::cloud::delete::item),
            )
            .service(
                web::scope("/server")
                    .service(crate::routes::server::get::item)
                    .service(crate::routes::server::get::list)
                    .service(crate::routes::server::update::item)
                    .service(crate::routes::server::delete::item),
            )
            .service(
                web::scope("/api/v1/agent")
                    .service(routes::agent::register_handler)
                    .service(routes::agent::wait_handler)
                    .service(routes::agent::report_handler),
            )
            .service(
                web::scope("/api/v1/commands")
                    .service(routes::command::create_handler)
                    .service(routes::command::list_handler)
                    .service(routes::command::get_handler)
                    .service(routes::command::cancel_handler),
            )
            .service(
                web::scope("/agreement")
                    .service(crate::routes::agreement::user_add_handler)
                    .service(crate::routes::agreement::get_handler)
                    .service(crate::routes::agreement::accept_handler),
            )
            .app_data(json_config.clone())
            .app_data(pg_pool.clone())
            .app_data(mq_manager.clone())
            .app_data(vault_client.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
