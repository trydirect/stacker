//! `agent-gateway` — standalone, independently-scalable MCP server for the
//! agent-facing tools (`resolve_image`, `deploy_ephemeral`). Boots the same
//! pools/config as the main server but serves only `/health` + `/mcp` via
//! `stacker::startup::run_agent_gateway`.
//!
//! Binds `AGENT_GATEWAY_BIND` (default `0.0.0.0:4600`) instead of the main
//! server's host/port, so it deploys and scales apart from the core API.

use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use stacker::banner;
use stacker::configuration::get_configuration;
use stacker::helpers::AgentPgPool;
use stacker::startup::run_agent_gateway;
use stacker::telemetry::{get_subscriber, init_subscriber};
use std::net::TcpListener;
use std::time::Duration;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    banner::print_banner();

    let subscriber = get_subscriber("agent-gateway".into(), "info".into());
    init_subscriber(subscriber);

    let settings = get_configuration().expect("Failed to read configuration.");

    let connect_options = PgConnectOptions::new()
        .host(&settings.database.host)
        .port(settings.database.port)
        .username(&settings.database.username)
        .password(&settings.database.password)
        .database(&settings.database.database_name)
        .ssl_mode(PgSslMode::Disable);

    let api_pool = PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .connect_with(connect_options.clone())
        .await
        .expect("Failed to connect to database (API pool).");

    let agent_pool_raw = PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .idle_timeout(Duration::from_secs(300))
        .max_lifetime(Duration::from_secs(1800))
        .connect_with(connect_options)
        .await
        .expect("Failed to connect to database (Agent pool).");
    let agent_pool = AgentPgPool::new(agent_pool_raw);

    let address = std::env::var("AGENT_GATEWAY_BIND").unwrap_or_else(|_| "0.0.0.0:4600".to_string());
    tracing::info!("Starting agent-gateway at {address}");
    let listener = TcpListener::bind(&address)
        .unwrap_or_else(|_| panic!("agent-gateway failed to bind to {address}"));

    run_agent_gateway(listener, api_pool, agent_pool, settings)
        .await?
        .await
}
