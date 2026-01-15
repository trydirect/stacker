use crate::configuration::Settings;
use crate::connectors;
use crate::health::{HealthChecker, HealthMetrics};
use crate::helpers;
use crate::mcp;
use crate::middleware;
use crate::routes;
use actix_cors::Cors;
use actix_web::{dev::Server, error, http, middleware, web, App, HttpServer};
use sqlx::{Pool, Postgres};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;
use tracing_actix_web::TracingLogger;

pub async fn run(
    listener: TcpListener,
    pg_pool: Pool<Postgres>,
    settings: Settings,
) -> Result<Server, std::io::Error> {
    let settings_arc = Arc::new(settings.clone());
    let pg_pool_arc = Arc::new(pg_pool.clone());

    let settings = web::Data::new(settings);
    let pg_pool = web::Data::new(pg_pool);

    let mq_manager = helpers::MqManager::try_new(settings.amqp.connection_string())?;
    let mq_manager = web::Data::new(mq_manager);

    let vault_client = helpers::VaultClient::new(&settings.vault);
    let vault_client = web::Data::new(vault_client);

    let oauth_http_client = reqwest::Client::builder()
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
    let oauth_http_client = web::Data::new(oauth_http_client);

    let oauth_cache = web::Data::new(middleware::authentication::OAuthCache::new(
        Duration::from_secs(60),
    ));

    // Initialize MCP tool registry
    let mcp_registry = Arc::new(mcp::ToolRegistry::new());
    let mcp_registry = web::Data::new(mcp_registry);

    // Initialize health checker and metrics
    let health_checker = Arc::new(HealthChecker::new(
        pg_pool_arc.clone(),
        settings_arc.clone(),
    ));
    let health_checker = web::Data::new(health_checker);

    let health_metrics = Arc::new(HealthMetrics::new(1000));
    let health_metrics = web::Data::new(health_metrics);

    // Initialize external service connectors (plugin pattern)
    // Connector handles category sync on startup
    let user_service_connector =
        connectors::init_user_service(&settings.connectors, pg_pool.clone());
    let dockerhub_connector = connectors::init_dockerhub(&settings.connectors).await;
    let install_service_connector: web::Data<Arc<dyn connectors::InstallServiceConnector>> =
        web::Data::new(Arc::new(connectors::InstallServiceClient));

    let authorization =
        middleware::authorization::try_new(settings.database.connection_string()).await?;
    let json_config = web::JsonConfig::default().error_handler(|err, _req| {
        //todo
        let msg: String = match err {
            error::JsonPayloadError::Deserialize(err) => format!(
                "{{\"kind\":\"deserialize\",\"line\":{}, \"column\":{}, \"msg\":\"{}\"}}",
                err.line(),
                err.column(),
                err
            ),
            _ => format!("{{\"kind\":\"other\",\"msg\":\"{}\"}}", err),
        };
        error::InternalError::new(msg, http::StatusCode::BAD_REQUEST).into()
    });
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .wrap(authorization.clone())
            .wrap(middleware::authentication::Manager::new())
            .wrap(middleware::Compress::default())
            .wrap(Cors::permissive())
            .app_data(health_checker.clone())
            .app_data(health_metrics.clone())
            .app_data(oauth_http_client.clone())
            .app_data(oauth_cache.clone())
            .service(
                web::scope("/health_check")
                    .service(routes::health_check)
                    .service(routes::health_metrics),
            )
            .service(
                web::scope("/client")
                    .service(routes::client::add_handler)
                    .service(routes::client::update_handler)
                    .service(routes::client::enable_handler)
                    .service(routes::client::disable_handler),
            )
            .service(web::scope("/test").service(routes::test::deploy::handler))
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
                web::scope("/dockerhub")
                    .service(crate::routes::dockerhub::search_namespaces)
                    .service(crate::routes::dockerhub::list_repositories)
                    .service(crate::routes::dockerhub::list_tags),
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
                    ),
            )
            .service(
                web::scope("/api")
                    .service(crate::routes::marketplace::categories::list_handler)
                    .service(
                        web::scope("/templates")
                            .service(crate::routes::marketplace::public::list_handler)
                            .service(crate::routes::marketplace::public::detail_handler)
                            .service(crate::routes::marketplace::creator::create_handler)
                            .service(crate::routes::marketplace::creator::update_handler)
                            .service(crate::routes::marketplace::creator::submit_handler)
                            .service(crate::routes::marketplace::creator::mine_handler),
                    )
                    .service(
                        web::scope("/v1/agent")
                            .service(routes::agent::register_handler)
                            .service(routes::agent::enqueue_handler)
                            .service(routes::agent::wait_handler)
                            .service(routes::agent::report_handler),
                    )
                    .service(
                        web::scope("/v1/deployments")
                            .service(routes::deployment::capabilities_handler),
                    )
                    .service(
                        web::scope("/v1/commands")
                            .service(routes::command::create_handler)
                            .service(routes::command::list_handler)
                            .service(routes::command::get_handler)
                            .service(routes::command::cancel_handler),
                    )
                    .service(
                        web::scope("/admin")
                            .service(
                                web::scope("/templates")
                                    .service(
                                        crate::routes::marketplace::admin::list_submitted_handler,
                                    )
                                    .service(crate::routes::marketplace::admin::approve_handler)
                                    .service(crate::routes::marketplace::admin::reject_handler),
                            )
                            .service(
                                web::scope("/marketplace")
                                    .service(crate::routes::marketplace::admin::list_plans_handler),
                            ),
                    ),
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
                web::scope("/agreement")
                    .service(crate::routes::agreement::user_add_handler)
                    .service(crate::routes::agreement::get_handler)
                    .service(crate::routes::agreement::accept_handler),
            )
            .service(web::resource("/mcp").route(web::get().to(mcp::mcp_websocket)))
            .app_data(json_config.clone())
            .app_data(pg_pool.clone())
            .app_data(mq_manager.clone())
            .app_data(vault_client.clone())
            .app_data(mcp_registry.clone())
            .app_data(user_service_connector.clone())
            .app_data(install_service_connector.clone())
            .app_data(dockerhub_connector.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
