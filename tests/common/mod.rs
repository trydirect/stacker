#![allow(
    dead_code,
    clippy::field_reassign_with_default,
    clippy::let_underscore_future
)]

use actix_web::{get, web, App, HttpServer, Responder};
use serde::Deserialize;
use sqlx::Row;
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

/// Long-lived runtime handle for test infrastructure (server + pool).
/// The runtime is leaked so it never drops, keeping the server alive and the
/// pool's background tasks running for the entire test process. Without this,
/// each `#[tokio::test]` creates its own runtime; the first test to initialize
/// the `OnceCell` spawns the server on ITS runtime, and when that test finishes
/// the runtime drops, killing the server and making pool connections stale.
fn infra_handle() -> tokio::runtime::Handle {
    static HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();
    HANDLE
        .get_or_init(|| {
            // multi_thread runtime has its own worker threads that keep running
            // even after the creating thread moves on (via std::mem::forget).
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .enable_all()
                .build()
                .expect("Failed to create test infrastructure runtime");
            let handle = rt.handle().clone();
            // Leak the runtime so its worker threads (pool connection manager,
            // server workers, Casbin reloader, etc.) stay alive for the process.
            std::mem::forget(rt);
            handle
        })
        .clone()
}

pub async fn spawn_app_with_configuration(mut configuration: Settings) -> Option<TestApp> {
    ensure_test_access_control_conf();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");

    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);
    configuration.database.database_name = uuid::Uuid::new_v4().to_string();
    let connection_string = configuration.database.connection_string();

    // Create pool and server on the long-lived infrastructure runtime so they
    // survive across #[tokio::test] runtime boundaries.
    let handle = infra_handle();
    let result = handle
        .spawn(async move {
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

            tokio::spawn(server);
            println!("Used Port: {}", port);

            Some(TestApp {
                address,
                db_pool: connection_pool,
                connection_string,
            })
        })
        .await;

    match result {
        Ok(app) => app,
        Err(err) => {
            eprintln!("Skipping tests: infrastructure task panicked: {}", err);
            None
        }
    }
}

pub async fn spawn_app() -> Option<TestApp> {
    let mut configuration = get_configuration().expect("Failed to get configuration");
    apply_test_database_env_overrides(&mut configuration);

    // Disable DockerHub connector in tests to skip Redis connection timeout
    if let Some(ref mut cfg) = configuration.connectors.dockerhub_service {
        cfg.enabled = false;
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind port for testing auth server");

    configuration.auth_url = format!(
        "http://127.0.0.1:{}/me",
        listener.local_addr().unwrap().port()
    );
    println!("Auth Server is running on: {}", configuration.auth_url);

    // Start mock auth server on the infrastructure runtime so it stays alive
    // across test function boundaries.
    let handle = infra_handle();
    handle.spawn(async move {
        mock_auth_server(listener).await;
    });
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

    // Disable DockerHub connector in tests to skip Redis connection timeout
    if let Some(ref mut cfg) = configuration.connectors.dockerhub_service {
        cfg.enabled = false;
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind port for testing auth server");

    configuration.auth_url = format!(
        "http://127.0.0.1:{}/me",
        listener.local_addr().unwrap().port()
    );

    let handle = infra_handle();
    handle.spawn(async move {
        mock_auth_server(listener).await;
    });
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
    // Disable DockerHub connector in tests to skip Redis connection timeout
    if let Some(ref mut cfg) = configuration.connectors.dockerhub_service {
        cfg.enabled = false;
    }

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

pub async fn seed_marketplace_template_ratings_for_vendor(pool: &PgPool, creator_user_id: &str) {
    let ratings_by_slug: std::collections::BTreeMap<&str, &[i32]> =
        std::collections::BTreeMap::from([
            // Internal rating storage is 0-10; public/template APIs expose 1-5 stars.
            ("wordpress-pro", &[10, 8][..]),
            ("postgres-backup", &[6][..]),
        ]);

    let templates = sqlx::query(
        r#"SELECT id, slug FROM stack_template
           WHERE creator_user_id = $1 AND status = 'approved'
           ORDER BY slug"#,
    )
    .bind(creator_user_id)
    .fetch_all(pool)
    .await
    .expect("Failed to fetch seeded marketplace templates for ratings");

    for (index, row) in templates.iter().enumerate() {
        let slug: String = row.get("slug");
        let product_id = 910_000 + index as i32;

        sqlx::query(
            r#"INSERT INTO product (id, obj_id, obj_type, created_at, updated_at)
               VALUES ($1, $1, 'marketplace_template', NOW(), NOW())
               ON CONFLICT (id) DO NOTHING"#,
        )
        .bind(product_id)
        .execute(pool)
        .await
        .expect("Failed to seed marketplace product for ratings");

        sqlx::query("UPDATE stack_template SET product_id = $2 WHERE slug = $1")
            .bind(&slug)
            .bind(product_id)
            .execute(pool)
            .await
            .expect("Failed to attach product_id to marketplace template");

        if let Some(ratings) = ratings_by_slug.get(slug.as_str()) {
            for (rating_index, rating_value) in ratings.iter().enumerate() {
                sqlx::query(
                    r#"INSERT INTO rating (
                        user_id,
                        obj_id,
                        category,
                        comment,
                        hidden,
                        rate,
                        created_at,
                        updated_at
                    )
                    VALUES ($1, $2, 'application', $3, false, $4, NOW(), NOW())
                    ON CONFLICT (user_id, obj_id, category) WHERE hidden = false DO UPDATE SET
                        comment = EXCLUDED.comment,
                        rate = EXCLUDED.rate,
                        updated_at = NOW()"#,
                )
                .bind(format!("rating-user-{}-{}", slug, rating_index))
                .bind(product_id)
                .bind(format!("rating {} for {}", rating_index, slug))
                .bind(*rating_value)
                .execute(pool)
                .await
                .expect("Failed to seed marketplace rating");
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Original infrastructure
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub async fn configure_database(config: &DatabaseSettings) -> Result<PgPool, sqlx::Error> {
    let mut connection = PgConnection::connect(&config.connection_string_without_db()).await?;

    connection
        .execute(format!(r#"CREATE DATABASE "{}""#, config.database_name).as_str())
        .await?;

    // Run migrations on a dedicated single-connection pool so the advisory lock
    // and any DDL catalog locks are fully released before the server pool is
    // created. This prevents PoolTimedOut when concurrent tests query the pool
    // immediately after server boot while migration connections are still in use.
    let migrate_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(120))
        .connect(&config.connection_string())
        .await?;
    sqlx::migrate!("./migrations").run(&migrate_pool).await?;
    migrate_pool.close().await;

    let connection_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(32)
        .acquire_timeout(std::time::Duration::from_secs(120))
        .connect(&config.connection_string())
        .await?;

    Ok(connection_pool)
}

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
    /// Database connection string. Tests that need a pool on their own runtime
    /// (to avoid cross-runtime PgPool issues) can create a fresh pool from this.
    pub connection_string: String,
}

/// Cached server info. Used by `get_or_init_app_fresh` to create a fresh
/// `PgPool` on the caller's runtime for each test, avoiding cross-runtime
/// pool issues.
pub struct TestAppConfig {
    pub address: String,
    pub connection_string: String,
}

/// Initialize a shared `TestApp` in the given `OnceCell`, booting the server
/// only once per test file. All tests in the file share the same server and
/// database. Tests must use unique identifiers (UUIDs) to avoid data conflicts.
///
/// Usage in a test file:
/// ```ignore
/// use tokio::sync::OnceCell;
/// static APP: OnceCell<common::TestApp> = OnceCell::const_new();
///
/// async fn app() -> &'static common::TestApp {
///     common::get_or_init_app(&APP).await.expect("Failed to start test app")
/// }
///
/// #[tokio::test]
/// async fn my_test() {
///     let app = app().await;
///     // ...
/// }
/// ```
pub async fn get_or_init_app(
    cell: &'static tokio::sync::OnceCell<TestApp>,
) -> Option<&'static TestApp> {
    cell.get_or_try_init(|| async { spawn_app().await.ok_or(()) })
        .await
        .ok()
}

/// Like `get_or_init_app` but creates a fresh `PgPool` on the caller's
/// runtime for each call. This avoids cross-runtime pool issues when
/// `#[tokio::test]` creates a new runtime per test function.
///
/// Usage:
/// ```ignore
/// use tokio::sync::OnceCell;
/// static APP_CONFIG: OnceCell<common::TestAppConfig> = OnceCell::const_new();
///
/// async fn app() -> common::TestApp {
///     common::get_or_init_app_fresh(&APP_CONFIG).await.expect("Failed to start test app")
/// }
/// ```
pub async fn get_or_init_app_fresh(
    cell: &'static tokio::sync::OnceCell<TestAppConfig>,
) -> Option<TestApp> {
    let config = cell
        .get_or_try_init(|| async {
            let app = spawn_app().await.ok_or(())?;
            Ok::<_, ()>(TestAppConfig {
                address: app.address,
                connection_string: app.connection_string,
            })
        })
        .await
        .ok()?;

    let db_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(120))
        .connect(&config.connection_string)
        .await
        .ok()?;

    Some(TestApp {
        address: config.address.clone(),
        db_pool,
        connection_string: config.connection_string.clone(),
    })
}

pub async fn get_or_init_two_user_app(
    cell: &'static tokio::sync::OnceCell<TwoUserTestApp>,
) -> Option<&'static TwoUserTestApp> {
    cell.get_or_try_init(|| async { spawn_app_two_users().await.ok_or(()) })
        .await
        .ok()
}

pub async fn get_or_init_vault_app(
    cell: &'static tokio::sync::OnceCell<TestAppWithVault>,
) -> Option<&'static TestAppWithVault> {
    cell.get_or_try_init(|| async { spawn_app_with_vault().await.ok_or(()) })
        .await
        .ok()
}

pub struct TestAppWithVault {
    pub address: String,
    pub db_pool: PgPool,
    pub vault_server: MockServer,
    pub connection_string: String,
}

/// Cached vault app info (address, DB connection string, and the mock Vault
/// server). Used by `get_or_init_vault_app_fresh` to create a fresh `PgPool`
/// on the caller's runtime for each test, avoiding cross-runtime pool issues
/// (see `TestAppConfig`/`get_or_init_app_fresh` above for the same pattern).
pub struct TestAppWithVaultShared {
    pub address: String,
    pub connection_string: String,
    pub vault_server: MockServer,
}

/// Vault test app with a `PgPool` created fresh on the caller's runtime.
/// `vault_server` is a `&'static MockServer` shared across tests in the file
/// (safe to reuse across runtimes since wiremock's `MockServer` runs its own
/// independent server/runtime internally).
pub struct TestAppWithVaultFresh {
    pub address: String,
    pub db_pool: PgPool,
    pub vault_server: &'static MockServer,
}

/// Dedicated background tokio runtime that outlives every individual
/// `#[tokio::test]` runtime. The actix server (and the `PgPool` it uses
/// internally) must be bootstrapped on a runtime that stays alive for the
/// whole test binary process — if it's bootstrapped on whichever ephemeral
/// per-test runtime happens to trigger the `OnceCell` first, that runtime is
/// dropped when that test returns, and the server's pool can no longer
/// establish new physical connections (acquire() then hangs until timeout).
static SERVER_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn server_runtime() -> &'static tokio::runtime::Runtime {
    SERVER_RUNTIME.get_or_init(|| {
        tokio::runtime::Runtime::new().expect("Failed to create dedicated server runtime")
    })
}

/// Like `get_or_init_vault_app` but creates a fresh `PgPool` on the caller's
/// runtime for each call. This avoids `PoolTimedOut` hangs caused by reusing
/// a single `PgPool` across the different tokio runtimes that `#[tokio::test]`
/// creates for each test function (a live connection/background task tied to
/// one test's runtime becomes unusable once that runtime is dropped).
///
/// Usage:
/// ```ignore
/// use tokio::sync::OnceCell;
/// static APP: OnceCell<common::TestAppWithVaultShared> = OnceCell::const_new();
///
/// async fn app() -> common::TestAppWithVaultFresh {
///     common::get_or_init_vault_app_fresh(&APP).await.expect("Failed to start test app")
/// }
/// ```
pub async fn get_or_init_vault_app_fresh(
    cell: &'static tokio::sync::OnceCell<TestAppWithVaultShared>,
) -> Option<TestAppWithVaultFresh> {
    let shared = cell
        .get_or_try_init(|| async {
            // Bootstrap the server and its pool on the persistent runtime
            // instead of the caller's (ephemeral) test runtime.
            let app = server_runtime()
                .spawn(spawn_app_with_vault())
                .await
                .ok()
                .flatten()
                .ok_or(())?;
            Ok::<_, ()>(TestAppWithVaultShared {
                address: app.address,
                connection_string: app.connection_string,
                vault_server: app.vault_server,
            })
        })
        .await
        .ok()?;

    let db_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(120))
        .connect(&shared.connection_string)
        .await
        .ok()?;

    Some(TestAppWithVaultFresh {
        address: shared.address.clone(),
        db_pool,
        vault_server: &shared.vault_server,
    })
}

/// Spawn the full app with a mock Vault server.
/// The returned `vault_server` is a wiremock MockServer — mount expectations on it
/// before calling API endpoints that touch Vault.
pub async fn spawn_app_with_vault() -> Option<TestAppWithVault> {
    let mut configuration = get_configuration().expect("Failed to get configuration");

    // Disable DockerHub connector in tests to skip Redis connection timeout
    if let Some(ref mut cfg) = configuration.connectors.dockerhub_service {
        cfg.enabled = false;
    }

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
    let connection_string = configuration.database.connection_string();

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
        connection_string,
    })
}

/// Insert a minimal project into the DB and return its id.
/// Required because server.project_id has a FK constraint to project(id).
pub async fn create_test_project(pool: &PgPool, user_id: &str) -> i32 {
    sqlx::query(
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, is_protected, created_at, updated_at)
        VALUES (gen_random_uuid(), $1, 'Test Project', '{}'::jsonb, '{}'::jsonb, false, NOW(), NOW())
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
