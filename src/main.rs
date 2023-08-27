use sqlx::PgPool;
use stacker::configuration::get_configuration;
use stacker::startup::run;
use stacker::telemetry::{get_subscriber, init_subscriber};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let subscriber = get_subscriber("stacker".into(), "info".into());
    init_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration.");
    let connection_pool = PgPool::connect(&configuration.database.connection_string())
        .await
        .expect("Failed to connect to database.");
    let address = format!("127.0.0.1:{}", configuration.application_port);
    tracing::info!("Start server at {:?}", &address);
    let listener = std::net::TcpListener::bind(address).expect(&format!(
        "failed to bind to {}",
        configuration.application_port
    ));

    run(listener, connection_pool)?.await
}

