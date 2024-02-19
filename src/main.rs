use sqlx::PgPool;
use stacker::configuration::get_configuration;
use stacker::startup::run;
use stacker::telemetry::{get_subscriber, init_subscriber};
use std::net::TcpListener;


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let subscriber = get_subscriber("stacker".into(), "info".into());
    init_subscriber(subscriber);

    let settings = get_configuration().expect("Failed to read configuration.");

    let pg_pool = PgPool::connect(&settings.database.connection_string())
        .await
        .expect("Failed to connect to database.");

    let address = format!("{}:{}", settings.app_host, settings.app_port);
    tracing::info!("Start server at {:?}", &address);
    let listener =
        TcpListener::bind(address).expect(&format!("failed to bind to {}", settings.app_port));

    run(listener, pg_pool, settings).await?.await
}
