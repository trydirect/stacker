use actix_web::{get, web, App, HttpServer, Responder};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use stacker::configuration::{get_configuration, DatabaseSettings};
use stacker::forms;
use stacker::helpers::AgentPgPool;
use std::net::TcpListener;
use std::time::Duration;
use tokio::time::sleep;

pub struct BddTestApp {
    pub address: String,
    pub db_pool: PgPool,
}

const POSTGRES_STARTUP_MAX_ATTEMPTS: u32 = 15;
const POSTGRES_STARTUP_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Spawn a test app instance for BDD tests.
/// Uses the same pattern as the existing integration tests.
pub async fn spawn_bdd_app() -> Option<BddTestApp> {
    let mut configuration = get_configuration().expect("Failed to get configuration");

    // Start mock auth server (token-aware: "user-b" → User B, anything else → User A)
    let auth_listener =
        TcpListener::bind("127.0.0.1:0").expect("Failed to bind port for BDD auth server");
    configuration.auth_url = format!(
        "http://127.0.0.1:{}/me",
        auth_listener.local_addr().unwrap().port()
    );
    let _ = tokio::spawn(mock_auth_server(auth_listener));
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Apply CI env overrides (PGHOST, PGPORT, PGUSER, PGPASSWORD) so BDD tests
    // work with the dynamic postgres port allocated by GitHub Actions services.
    if let Ok(host) = std::env::var("PGHOST") {
        configuration.database.host = host;
    }
    if let Ok(port) = std::env::var("PGPORT") {
        if let Ok(parsed) = port.parse::<u16>() {
            configuration.database.port = parsed;
        }
    }
    if let Ok(username) = std::env::var("PGUSER") {
        configuration.database.username = username;
    }
    if let Ok(password) = std::env::var("PGPASSWORD") {
        configuration.database.password = password;
    }

    // Unique database per BDD run
    configuration.database.database_name = format!("bdd_{}", uuid::Uuid::new_v4());

    // Increase client limit for BDD tests (multiple scenarios create clients)
    configuration.max_clients_number = 100;

    // Set internal services access key for audit ingest tests
    std::env::set_var("INTERNAL_SERVICES_ACCESS_KEY", "bdd-internal-key");

    let connection_pool = match configure_database_with_retry(&configuration.database).await {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("BDD: Skipping — failed to connect to postgres: {}", err);
            return None;
        }
    };

    let app_listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind port for BDD app");
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
    .expect("Failed to start BDD app server.");
    let _ = tokio::spawn(server);

    Some(BddTestApp {
        address,
        db_pool: connection_pool,
    })
}

async fn configure_database(config: &DatabaseSettings) -> Result<PgPool, sqlx::Error> {
    let mut connection = PgConnection::connect(&config.connection_string_without_db()).await?;
    connection
        .execute(format!(r#"CREATE DATABASE "{}""#, config.database_name).as_str())
        .await?;
    let pool = PgPool::connect(&config.connection_string()).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

async fn configure_database_with_retry(config: &DatabaseSettings) -> Result<PgPool, sqlx::Error> {
    let mut last_err = None;

    for attempt in 1..=POSTGRES_STARTUP_MAX_ATTEMPTS {
        match configure_database(config).await {
            Ok(pool) => return Ok(pool),
            Err(err) => {
                if attempt == POSTGRES_STARTUP_MAX_ATTEMPTS
                    || !is_retryable_postgres_startup_error(&err)
                {
                    return Err(err);
                }

                eprintln!(
                    "BDD: Waiting for postgres startup (attempt {attempt}/{POSTGRES_STARTUP_MAX_ATTEMPTS}): {err}"
                );
                last_err = Some(err);
                sleep(POSTGRES_STARTUP_RETRY_DELAY).await;
            }
        }
    }

    Err(last_err.expect("BDD configure_database_with_retry exhausted without error"))
}

fn is_retryable_postgres_startup_error(err: &sqlx::Error) -> bool {
    if matches!(
        err,
        sqlx::Error::Io(_)
            | sqlx::Error::PoolTimedOut
            | sqlx::Error::PoolClosed
            | sqlx::Error::Tls(_)
            | sqlx::Error::Protocol(_)
    ) {
        return true;
    }

    let message = err.to_string().to_ascii_lowercase();
    message.contains("connection refused")
        || message.contains("database system is starting up")
        || message.contains("the database system is starting up")
        || message.contains("could not connect")
        || message.contains("timeout")
        || message.contains("timed out")
        || message.contains("server closed the connection unexpectedly")
}

// Token-aware mock auth server: "user-b" token → User B, anything else → User A
pub const USER_A_ID: &str = "test_user_id";
pub const USER_A_EMAIL: &str = "test@example.com";
pub const USER_B_ID: &str = "other_user_id";
pub const USER_B_EMAIL: &str = "other@example.com";

#[get("")]
async fn mock_auth(req: actix_web::HttpRequest) -> actix_web::Result<impl Responder> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_user_b = auth_header.contains("user-b");
    let is_admin = auth_header.contains("admin");

    let mut user = forms::user::User::default();
    if is_user_b {
        user.id = USER_B_ID.to_string();
        user.email = USER_B_EMAIL.to_string();
        user.role = "group_user".to_string();
    } else if is_admin {
        user.id = USER_A_ID.to_string();
        user.email = USER_A_EMAIL.to_string();
        user.role = "group_admin".to_string();
    } else {
        user.id = USER_A_ID.to_string();
        user.email = USER_A_EMAIL.to_string();
        user.role = "group_user".to_string();
    }
    user.email_confirmed = true;

    Ok(web::Json(forms::user::UserForm { user }))
}

async fn mock_auth_server(listener: TcpListener) {
    HttpServer::new(|| App::new().service(web::scope("/me").service(mock_auth)))
        .listen(listener)
        .unwrap()
        .run()
        .await
        .unwrap();
}

// ─── Test data helpers ───────────────────────────────────────────

/// Insert a minimal project and return its id.
pub async fn create_test_project(pool: &PgPool, user_id: &str) -> i32 {
    sqlx::query_scalar(
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
        VALUES (gen_random_uuid(), $1, 'BDD Test Project', '{}'::jsonb, '{}'::jsonb, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("Failed to insert BDD test project")
}

/// Insert a test cloud credential and return its id.
pub async fn create_test_cloud(pool: &PgPool, user_id: &str, provider: &str) -> i32 {
    sqlx::query_scalar(
        r#"INSERT INTO cloud (user_id, name, provider, cloud_token, save_token, created_at, updated_at)
        VALUES ($1, 'BDD Cloud', $2, 'bdd-token', true, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(provider)
    .fetch_one(pool)
    .await
    .expect("Failed to insert BDD test cloud")
}

/// Insert a test server and return its id.
pub async fn create_test_server(pool: &PgPool, user_id: &str, project_id: i32) -> i32 {
    sqlx::query_scalar(
        r#"INSERT INTO server (user_id, project_id, connection_mode, key_status, created_at, updated_at)
        VALUES ($1, $2, 'ssh', 'none', NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
    .expect("Failed to insert BDD test server")
}
