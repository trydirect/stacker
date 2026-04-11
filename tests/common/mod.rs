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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Multi-user test infrastructure for IDOR security tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// User A (default) identifiers — the "owner" in IDOR tests
pub const USER_A_ID: &str = "test_user_id";
pub const USER_A_EMAIL: &str = "test@example.com";
pub const USER_A_TOKEN: &str = "user-a-token";

/// User B identifiers — the "attacker" in IDOR tests
pub const USER_B_ID: &str = "other_user_id";
pub const USER_B_EMAIL: &str = "other@example.com";
pub const USER_B_TOKEN: &str = "user-b-token";

pub struct TwoUserTestApp {
    pub address: String,
    pub db_pool: PgPool,
}

/// Spawn an app with a token-aware mock auth server.
/// - Bearer token containing "user-b" → returns User B (other_user_id)
/// - Any other Bearer token → returns User A (test_user_id)
pub async fn spawn_app_two_users() -> Option<TwoUserTestApp> {
    let mut configuration = get_configuration().expect("Failed to get configuration");

    let auth_listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind port for testing auth server");

    configuration.auth_url = format!(
        "http://127.0.0.1:{}/me",
        auth_listener.local_addr().unwrap().port()
    );

    let _ = tokio::spawn(mock_auth_server_two_users(auth_listener));
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

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
    let server = stacker::startup::run(
        app_listener,
        connection_pool.clone(),
        agent_pool,
        configuration,
    )
    .await
    .expect("Failed to bind address.");
    let _ = tokio::spawn(server);

    Some(TwoUserTestApp {
        address,
        db_pool: connection_pool,
    })
}

/// Token-aware mock auth: inspects the Authorization header to return different users.
#[get("")]
async fn mock_auth_two_users(req: actix_web::HttpRequest) -> actix_web::Result<impl Responder> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_user_b = auth_header.contains("user-b");

    let mut user = forms::user::User::default();
    if is_user_b {
        user.id = USER_B_ID.to_string();
        user.email = USER_B_EMAIL.to_string();
    } else {
        user.id = USER_A_ID.to_string();
        user.email = USER_A_EMAIL.to_string();
    }
    user.role = "group_user".to_string();
    user.email_confirmed = true;

    Ok(web::Json(forms::user::UserForm { user }))
}

async fn mock_auth_server_two_users(listener: TcpListener) {
    HttpServer::new(|| App::new().service(web::scope("/me").service(mock_auth_two_users)))
        .listen(listener)
        .unwrap()
        .run()
        .await
        .unwrap();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test data helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Insert a minimal cloud credential into the DB and return its id.
pub async fn create_test_cloud(pool: &PgPool, user_id: &str, name: &str, provider: &str) -> i32 {
    sqlx::query(
        r#"INSERT INTO cloud (user_id, name, provider, cloud_token, save_token, created_at, updated_at)
        VALUES ($1, $2, $3, 'test-token-encrypted', true, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(name)
    .bind(provider)
    .fetch_one(pool)
    .await
    .map(|row| {
        use sqlx::Row;
        row.get::<i32, _>("id")
    })
    .expect("Failed to insert test cloud")
}

/// Insert a minimal deployment into the DB and return its id.
pub async fn create_test_deployment(
    pool: &PgPool,
    user_id: &str,
    project_id: i32,
    deployment_hash: &str,
) -> i32 {
    sqlx::query(
        r#"INSERT INTO deployment (project_id, deployment_hash, user_id, status, runtime, created_at, updated_at)
        VALUES ($1, $2, $3, 'running', 'runc', NOW(), NOW())
        RETURNING id"#,
    )
    .bind(project_id)
    .bind(deployment_hash)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map(|row| {
        use sqlx::Row;
        row.get::<i32, _>("id")
    })
    .expect("Failed to insert test deployment")
}

/// Insert a test client record and return its id.
pub async fn create_test_client(pool: &PgPool, user_id: &str) -> i32 {
    sqlx::query(
        r#"INSERT INTO client (user_id, secret, enabled, created_at, updated_at)
        VALUES ($1, 'test-client-secret', true, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map(|row| {
        use sqlx::Row;
        row.get::<i32, _>("id")
    })
    .expect("Failed to insert test client")
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Original infrastructure
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

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
    let server = stacker::startup::run(
        app_listener,
        connection_pool.clone(),
        agent_pool,
        configuration,
    )
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

async fn mock_auth_server(listener: TcpListener) {
    HttpServer::new(|| App::new().service(web::scope("/me").service(mock_auth)))
        .listen(listener)
        .unwrap()
        .run()
        .await
        .unwrap();
}
