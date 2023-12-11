use actix_web::{get, web, App, HttpServer, Responder};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use stacker::configuration::{get_configuration, DatabaseSettings, Settings};
use stacker::forms;
use std::net::TcpListener;

pub async fn spawn_app_with_configuration(mut configuration: Settings) -> TestApp {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");

    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);
    configuration.database.database_name = uuid::Uuid::new_v4().to_string();

    let connection_pool = configure_database(&configuration.database).await;

    let server = stacker::startup::run(listener, connection_pool.clone(), configuration)
        .await
        .expect("Failed to bind address.");

    let _ = tokio::spawn(server);
    println!("Used Port: {}", port);

    TestApp {
        address,
        db_pool: connection_pool,
    }
}

pub async fn spawn_app() -> TestApp {
    let mut configuration = get_configuration().expect("Failed to get configuration");

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind port for testing auth server");

    configuration.auth_url = format!(
        "http://127.0.0.1:{}/me",
        listener.local_addr().unwrap().port()
    );
    println!("Auth Server is running on: {}", configuration.auth_url);

    let handle = tokio::spawn(mock_auth_server(listener));
    handle.await.expect("Auth Server can not be started");

    spawn_app_with_configuration(configuration).await
}

pub async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect(&config.connection_string_without_db())
        .await
        .expect("Failed to connect to postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}""#, config.database_name).as_str())
        .await
        .expect("Failed to create database");

    let connection_pool = PgPool::connect(&config.connection_string())
        .await
        .expect("Failed to connect to database pool");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate database");

    connection_pool
}

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}

#[get("")]
async fn mock_auth() -> actix_web::Result<impl Responder> {
    println!("Starting auth server in test mode ...");
    // 1. set user id
    // 2. add token to header / hardcoded
    Ok(web::Json(forms::user::UserForm::default()))
}

async fn mock_auth_server(listener: TcpListener) -> actix_web::dev::Server {
    HttpServer::new(|| App::new().service(web::scope("/me").service(mock_auth)))
        .listen(listener)
        .unwrap()
        .run()
}
