use actix_web::{get, web, App, HttpServer, Responder};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use stacker::configuration::{get_configuration, DatabaseSettings, Settings};
use stacker::forms;
use stacker::helpers::AgentPgPool;
use std::net::TcpListener;
use wiremock::MockServer;

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

    let agent_pool = AgentPgPool::new(connection_pool.clone());
    let server =
        stacker::startup::run(listener, connection_pool.clone(), agent_pool, configuration)
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

    sqlx::migrate!("./migrations").run(&connection_pool).await?;

    Ok(connection_pool)
}

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}

pub struct TestAppWithVault {
    pub address: String,
    pub db_pool: PgPool,
    pub vault_server: MockServer,
}

/// Spawn the full app with a mock Vault server.
/// The returned `vault_server` is a wiremock MockServer — mount expectations on it
/// before calling API endpoints that touch Vault.
pub async fn spawn_app_with_vault() -> Option<TestAppWithVault> {
    let mut configuration = get_configuration().expect("Failed to get configuration");

    // Mock auth server
    let auth_listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind port for testing auth server");
    configuration.auth_url = format!(
        "http://127.0.0.1:{}/me",
        auth_listener.local_addr().unwrap().port()
    );
    let _ = tokio::spawn(mock_auth_server(auth_listener));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Mock Vault server
    let vault_server = MockServer::start().await;
    configuration.vault.address = vault_server.uri();
    configuration.vault.token = "test-vault-token".to_string();
    configuration.vault.api_prefix = "v1".to_string();
    configuration.vault.ssh_key_path_prefix = Some("users".to_string());

    configuration.database.database_name = uuid::Uuid::new_v4().to_string();

    let connection_pool = match configure_database(&configuration.database).await {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("Skipping tests: failed to connect to postgres: {}", err);
            return None;
        }
    };

    let app_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind app port");
    let port = app_listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);

    let agent_pool = AgentPgPool::new(connection_pool.clone());
    let server = stacker::startup::run(app_listener, connection_pool.clone(), agent_pool, configuration)
        .await
        .expect("Failed to bind address.");
    let _ = tokio::spawn(server);

    Some(TestAppWithVault {
        address,
        db_pool: connection_pool,
        vault_server,
    })
}

/// Insert a minimal project into the DB and return its id.
/// Required because server.project_id has a FK constraint to project(id).
pub async fn create_test_project(pool: &PgPool, user_id: &str) -> i32 {
    sqlx::query(
        r#"INSERT INTO project (stack_id, user_id, name, body, created_at, updated_at)
        VALUES (gen_random_uuid(), $1, 'Test Project', '{}', NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map(|row| {
        use sqlx::Row;
        row.get::<i32, _>("id")
    })
    .expect("Failed to insert test project")
}

/// Insert a test server with specific SSH key state and return its id.
pub async fn create_test_server(
    pool: &PgPool,
    user_id: &str,
    project_id: i32,
    key_status: &str,
    vault_key_path: Option<&str>,
) -> i32 {
    sqlx::query(
        r#"INSERT INTO server (user_id, project_id, connection_mode, key_status, vault_key_path, created_at, updated_at)
        VALUES ($1, $2, 'ssh', $3, $4, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(project_id)
    .bind(key_status)
    .bind(vault_key_path)
    .fetch_one(pool)
    .await
    .map(|row| {
        use sqlx::Row;
        row.get::<i32, _>("id")
    })
    .expect("Failed to insert test server")
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
