use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use stacker::banner;
use stacker::configuration::get_configuration;
use stacker::startup::run;
use stacker::telemetry::{get_subscriber, init_subscriber};
use std::net::TcpListener;
use std::time::Duration;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Display banner
    banner::print_banner();

    let subscriber = get_subscriber("stacker".into(), "info".into());
    init_subscriber(subscriber);

    let settings = get_configuration().expect("Failed to read configuration.");

    tracing::info!(
        db_host = %settings.database.host,
        db_port = settings.database.port,
        db_name = %settings.database.database_name,
        "Connecting to PostgreSQL"
    );

    let connect_options = PgConnectOptions::new()
        .host(&settings.database.host)
        .port(settings.database.port)
        .username(&settings.database.username)
        .password(&settings.database.password)
        .database(&settings.database.database_name)
        .ssl_mode(PgSslMode::Disable);

    let pg_pool = PgPoolOptions::new()
        .max_connections(50) // Increased from 5 to handle concurrent agent polling + regular requests
        .min_connections(5) // Keep minimum pool size for quick response
        .acquire_timeout(Duration::from_secs(10)) // Reduced from 30s - fail faster if pool exhausted
        .idle_timeout(Duration::from_secs(600)) // Close idle connections after 10 minutes
        .max_lifetime(Duration::from_secs(1800)) // Recycle connections after 30 minutes
        .connect_with(connect_options)
        .await
        .expect("Failed to connect to database.");

    let address = format!("{}:{}", settings.app_host, settings.app_port);
    banner::print_startup_info(&settings.app_host, settings.app_port);
    tracing::info!("Start server at {:?}", &address);
    let listener =
        TcpListener::bind(address).expect(&format!("failed to bind to {}", settings.app_port));

    run(listener, pg_pool, settings).await?.await
}
