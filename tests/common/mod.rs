use actix_web::{get, web, App, HttpServer, Responder};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use stacker::configuration::{get_configuration, DatabaseSettings, Settings};
use stacker::forms;
use std::net::TcpListener;

pub async fn spawn_app_with_configuration(mut configuration: Settings) -> Option<TestApp> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");

    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);
    configuration.database.database_name = uuid::Uuid::new_v4().to_string();

    let connection_pool = match configure_database(&configuration.database).await {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("Skipping tests: failed to connect to postgres: {}", err);
            return None;
        }
    };

    let server = stacker::startup::run(listener, connection_pool.clone(), configuration)
        .await
        .expect("Failed to bind address.");

    let _ = tokio::spawn(server);
    println!("Used Port: {}", port);

    Some(TestApp {
        address,
        db_pool: connection_pool,
    })
}

pub async fn spawn_app() -> Option<TestApp> {
    let mut configuration = get_configuration().expect("Failed to get configuration");

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind port for testing auth server");

    configuration.auth_url = format!(
        "http://127.0.0.1:{}/me",
        listener.local_addr().unwrap().port()
    );
    println!("Auth Server is running on: {}", configuration.auth_url);

    // Start mock auth server in background; do not await the JoinHandle
    let _ = tokio::spawn(mock_auth_server(listener));
    // Give the mock server a brief moment to start listening
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Sanity check: attempt to hit the mock auth endpoint
    if let Ok(resp) = reqwest::Client::new()
        .get(configuration.auth_url.clone())
        .send()
        .await
    {
        println!("Mock auth sanity check status: {}", resp.status());
    } else {
        println!("Mock auth sanity check failed: unable to connect");
    }

    spawn_app_with_configuration(configuration).await
}

pub async fn configure_database(config: &DatabaseSettings) -> Result<PgPool, sqlx::Error> {
    let mut connection = PgConnection::connect(&config.connection_string_without_db()).await?;

    connection
        .execute(format!(r#"CREATE DATABASE "{}""#, config.database_name).as_str())
        .await?;

    let connection_pool = PgPool::connect(&config.connection_string()).await?;

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await?;

    Ok(connection_pool)
}

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}

#[get("")]
async fn mock_auth() -> actix_web::Result<impl Responder> {
    println!("Mock auth endpoint called - returning test user");

    // Return a test user with proper fields
    let mut user = forms::user::User::default();
    user.id = "test_user_id".to_string();
    user.email = "test@example.com".to_string();
    user.role = "group_user".to_string();
    user.email_confirmed = true;

    let user_form = forms::user::UserForm { user };

    Ok(web::Json(user_form))
}

async fn mock_auth_server(listener: TcpListener) -> actix_web::dev::Server {
    HttpServer::new(|| App::new().service(web::scope("/me").service(mock_auth)))
        .listen(listener)
        .unwrap()
        .run()
}
