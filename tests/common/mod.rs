#![allow(
    dead_code,
    clippy::field_reassign_with_default,
    clippy::let_underscore_future
)]

use actix_web::{get, web, App, HttpServer, Responder};
use serde::Deserialize;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use stacker::configuration::{get_configuration, DatabaseSettings, Settings};
use stacker::connectors::config::UserServiceConfig;
use stacker::forms;
use stacker::helpers::AgentPgPool;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use wiremock::MockServer;

static ACCESS_CONTROL_CONF_READY: OnceLock<()> = OnceLock::new();

pub async fn spawn_app_with_configuration(mut configuration: Settings) -> Option<TestApp> {
    ensure_test_access_control_conf();

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
    apply_test_database_env_overrides(&mut configuration);

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

pub async fn spawn_app_with_test_auth_configuration(
    mut configuration: Settings,
) -> Option<TestApp> {
    apply_test_database_env_overrides(&mut configuration);

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind port for testing auth server");

    configuration.auth_url = format!(
        "http://127.0.0.1:{}/me",
        listener.local_addr().unwrap().port()
    );

    let _ = tokio::spawn(mock_auth_server(listener));
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    spawn_app_with_configuration(configuration).await
}

fn apply_test_database_env_overrides(configuration: &mut Settings) {
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
}

fn ensure_test_access_control_conf() {
    ACCESS_CONTROL_CONF_READY.get_or_init(|| {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        std::env::set_current_dir(manifest_dir).expect("Failed to switch tests to repo root");

        let primary = manifest_dir.join("access_control.conf");
        if primary.exists() {
            return;
        }

        let dist = manifest_dir.join("access_control.conf.dist");
        if dist.exists() {
            std::fs::copy(dist, primary)
                .expect("Failed to provision access_control.conf for tests");
        }
    });
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
    let configuration = get_configuration().expect("Failed to get configuration");
    spawn_app_two_users_with_configuration(configuration).await
}

pub async fn spawn_app_two_users_with_user_service(
    user_service_base_url: &str,
) -> Option<TwoUserTestApp> {
    let mut configuration = get_configuration().expect("Failed to get configuration");
    configuration.connectors.user_service = Some(UserServiceConfig {
        enabled: true,
        base_url: user_service_base_url.trim_end_matches('/').to_string(),
        timeout_secs: 10,
        retry_attempts: 1,
        auth_token: None,
    });
    spawn_app_two_users_with_configuration(configuration).await
}

pub async fn spawn_app_two_users_with_configuration(
    mut configuration: Settings,
) -> Option<TwoUserTestApp> {
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
    let cloud_form = forms::CloudForm {
        user_id: Some(user_id.to_string()),
        project_id: None,
        name: Some(name.to_string()),
        provider: provider.to_string(),
        cloud_token: Some("test-cloud-token".to_string()),
        cloud_key: None,
        cloud_secret: None,
        save_token: Some(true),
    };

    let cloud: stacker::models::Cloud = (&cloud_form).into();
    sqlx::query(
        r#"INSERT INTO cloud (
            user_id,
            name,
            provider,
            cloud_token,
            cloud_key,
            cloud_secret,
            save_token,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(cloud.user_id)
    .bind(cloud.name)
    .bind(cloud.provider)
    .bind(cloud.cloud_token)
    .bind(cloud.cloud_key)
    .bind(cloud.cloud_secret)
    .bind(cloud.save_token)
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
        r#"INSERT INTO deployment (
            project_id,
            deployment_hash,
            user_id,
            metadata,
            status,
            runtime,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, '{}'::jsonb, 'running', 'runc', NOW(), NOW())
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

#[derive(Debug, Clone, Deserialize)]
pub struct MarketplaceVendorFixture {
    pub creator_user_id: String,
    pub public_slug: String,
    pub display_name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub website_url: Option<String>,
    pub verification_status: String,
    pub onboarding_status: String,
    pub payouts_enabled: bool,
    pub payout_provider: Option<String>,
    pub payout_account_ref: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketplaceTemplateFixture {
    pub creator_user_id: String,
    pub creator_name: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub tags: serde_json::Value,
    pub tech_stack: serde_json::Value,
    pub vendor_url: Option<String>,
}

fn shared_fixtures_root() -> PathBuf {
    if let Ok(path) = std::env::var("SHARED_FIXTURES_DIR") {
        return PathBuf::from(path);
    }

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .join("config/shared-fixtures")
}

fn read_marketplace_shared_fixture(file_name: &str) -> String {
    let shared_path = shared_fixtures_root().join("marketplace").join(file_name);
    if shared_path.exists() {
        return std::fs::read_to_string(&shared_path).unwrap_or_else(|err| {
            panic!(
                "Failed to read shared fixture {}: {}",
                shared_path.display(),
                err
            )
        });
    }

    let fallback_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/shared/marketplace")
        .join(file_name);
    std::fs::read_to_string(&fallback_path).unwrap_or_else(|err| {
        panic!(
            "Failed to read fallback fixture {}: {}",
            fallback_path.display(),
            err
        )
    })
}

pub fn marketplace_vendor_fixtures() -> Vec<MarketplaceVendorFixture> {
    serde_json::from_str(&read_marketplace_shared_fixture("vendors.json"))
        .expect("marketplace vendor fixtures should be valid JSON")
}

pub fn marketplace_template_fixtures() -> Vec<MarketplaceTemplateFixture> {
    serde_json::from_str(&read_marketplace_shared_fixture("templates.json"))
        .expect("marketplace template fixtures should be valid JSON")
}

pub async fn seed_marketplace_vendor_fixture(
    pool: &PgPool,
    public_slug: &str,
) -> MarketplaceVendorFixture {
    let vendor = marketplace_vendor_fixtures()
        .into_iter()
        .find(|fixture| fixture.public_slug == public_slug)
        .unwrap_or_else(|| panic!("Unknown marketplace vendor fixture: {}", public_slug));

    sqlx::query(
        r#"INSERT INTO marketplace_vendor_profile (
            creator_user_id,
            public_slug,
            display_name,
            bio,
            avatar_url,
            website_url,
            verification_status,
            onboarding_status,
            payouts_enabled,
            payout_provider,
            payout_account_ref,
            metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (creator_user_id) DO UPDATE SET
            public_slug = EXCLUDED.public_slug,
            display_name = EXCLUDED.display_name,
            bio = EXCLUDED.bio,
            avatar_url = EXCLUDED.avatar_url,
            website_url = EXCLUDED.website_url,
            verification_status = EXCLUDED.verification_status,
            onboarding_status = EXCLUDED.onboarding_status,
            payouts_enabled = EXCLUDED.payouts_enabled,
            payout_provider = EXCLUDED.payout_provider,
            payout_account_ref = EXCLUDED.payout_account_ref,
            metadata = EXCLUDED.metadata,
            updated_at = NOW()"#,
    )
    .bind(&vendor.creator_user_id)
    .bind(&vendor.public_slug)
    .bind(&vendor.display_name)
    .bind(&vendor.bio)
    .bind(&vendor.avatar_url)
    .bind(&vendor.website_url)
    .bind(&vendor.verification_status)
    .bind(&vendor.onboarding_status)
    .bind(vendor.payouts_enabled)
    .bind(&vendor.payout_provider)
    .bind(&vendor.payout_account_ref)
    .bind(&vendor.metadata)
    .execute(pool)
    .await
    .expect("Failed to seed marketplace vendor fixture");

    vendor
}

pub async fn seed_marketplace_template_fixtures_for_vendor(
    pool: &PgPool,
    creator_user_id: &str,
) -> Vec<MarketplaceTemplateFixture> {
    let templates = marketplace_template_fixtures()
        .into_iter()
        .filter(|fixture| fixture.creator_user_id == creator_user_id)
        .collect::<Vec<_>>();

    for template in &templates {
        sqlx::query(
            r#"INSERT INTO stack_template (
                creator_user_id,
                creator_name,
                name,
                slug,
                status,
                short_description,
                long_description,
                tags,
                tech_stack,
                vendor_url,
                approved_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                CASE WHEN $5 = 'approved' THEN NOW() ELSE NULL END
            )
            ON CONFLICT (slug) DO UPDATE SET
                creator_user_id = EXCLUDED.creator_user_id,
                creator_name = EXCLUDED.creator_name,
                name = EXCLUDED.name,
                status = EXCLUDED.status,
                short_description = EXCLUDED.short_description,
                long_description = EXCLUDED.long_description,
                tags = EXCLUDED.tags,
                tech_stack = EXCLUDED.tech_stack,
                vendor_url = EXCLUDED.vendor_url,
                approved_at = EXCLUDED.approved_at"#,
        )
        .bind(&template.creator_user_id)
        .bind(&template.creator_name)
        .bind(&template.name)
        .bind(&template.slug)
        .bind(&template.status)
        .bind(&template.short_description)
        .bind(&template.long_description)
        .bind(&template.tags)
        .bind(&template.tech_stack)
        .bind(&template.vendor_url)
        .execute(pool)
        .await
        .expect("Failed to seed marketplace template fixture");
    }

    templates
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
    configuration.connectors.install_service =
        Some(stacker::connectors::InstallServiceConfig { enabled: false });

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
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
        VALUES (gen_random_uuid(), $1, 'Test Project', '{}'::jsonb, '{}'::jsonb, NOW(), NOW())
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
