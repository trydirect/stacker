mod common;

use chrono::Utc;
use sqlx::Row;
use stacker::db;
use stacker::models::Agent;
use tokio::sync::OnceCell;

static APP: OnceCell<common::TestAppWithVaultShared> = OnceCell::const_new();

async fn app() -> common::TestAppWithVaultFresh {
    common::get_or_init_vault_app_fresh(&APP)
        .await
        .expect("Failed to start test app")
}

async fn create_project(pool: &sqlx::PgPool, user_id: &str) -> i32 {
    sqlx::query_scalar::<_, i32>(
        "INSERT INTO project (stack_id, user_id, name, metadata, created_at, updated_at)
         VALUES (gen_random_uuid(), $1, $2, '{}'::jsonb, NOW(), NOW())
         RETURNING id",
    )
    .bind(user_id)
    .bind(format!("sweep-test-{}", uuid::Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("Failed to create project")
}

async fn create_deployment(
    pool: &sqlx::PgPool,
    project_id: i32,
    user_id: &str,
    deployment_hash: &str,
    deleted: bool,
) {
    sqlx::query(
        "INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, deleted, created_at, updated_at)
         VALUES ($1, $2, $3, '{}'::jsonb, 'running', $4, NOW(), NOW())",
    )
    .bind(project_id)
    .bind(deployment_hash)
    .bind(user_id)
    .bind(deleted)
    .execute(pool)
    .await
    .expect("Failed to create deployment");
}

async fn insert_agent(pool: &sqlx::PgPool, agent: &Agent) {
    sqlx::query(
        "INSERT INTO agents (id, deployment_hash, capabilities, version, system_info,
                             last_heartbeat, status, token_hash, token_hash_updated_at,
                             created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(agent.id)
    .bind(&agent.deployment_hash)
    .bind(&agent.capabilities)
    .bind(&agent.version)
    .bind(&agent.system_info)
    .bind(agent.last_heartbeat)
    .bind(&agent.status)
    .bind(&agent.token_hash)
    .bind(agent.token_hash_updated_at)
    .bind(agent.created_at)
    .bind(agent.updated_at)
    .execute(pool)
    .await
    .expect("Failed to insert agent");
}

async fn agent_exists(pool: &sqlx::PgPool, agent_id: uuid::Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1)")
        .bind(agent_id)
        .fetch_one(pool)
        .await
        .expect("Failed to check agent existence")
}

async fn count_agents(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agents")
        .fetch_one(pool)
        .await
        .expect("Failed to count agents")
}

/// Case 1: deployment deleted, no activity → row deleted
#[tokio::test]
async fn sweep_dead_deletes_agent_with_deleted_deployment() {
    let app = app().await;
    let project_id = create_project(&app.db_pool, "test_user_id").await;
    let hash = format!("deployment_{}", uuid::Uuid::new_v4());
    create_deployment(&app.db_pool, project_id, "test_user_id", &hash, true).await;

    let mut agent = Agent::new(hash);
    agent.last_heartbeat = Some(Utc::now() - chrono::Duration::days(60));
    insert_agent(&app.db_pool, &agent).await;

    let removed = db::agent::sweep_dead(&app.db_pool, 30).await.unwrap();
    assert_eq!(removed, 1);
    assert!(!agent_exists(&app.db_pool, agent.id).await);
}

/// Case 2: deployment deleted, but fresh audit_log entry → row preserved
#[tokio::test]
async fn sweep_dead_preserves_agent_with_recent_audit_log() {
    let app = app().await;
    let project_id = create_project(&app.db_pool, "test_user_id").await;
    let hash = format!("deployment_{}", uuid::Uuid::new_v4());
    create_deployment(&app.db_pool, project_id, "test_user_id", &hash, true).await;

    let mut agent = Agent::new(hash.clone());
    agent.last_heartbeat = Some(Utc::now() - chrono::Duration::days(60));
    insert_agent(&app.db_pool, &agent).await;

    // Insert a recent audit_log entry (auth_failure — agent is alive but failing auth)
    sqlx::query(
        "INSERT INTO audit_log (id, agent_id, deployment_hash, action, status, created_at)
         VALUES ($1, $2, $3, 'auth_failure', 'failure', NOW())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(agent.id)
    .bind(&hash)
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert audit log");

    let removed = db::agent::sweep_dead(&app.db_pool, 30).await.unwrap();
    assert_eq!(removed, 0);
    assert!(agent_exists(&app.db_pool, agent.id).await);
}

/// Case 3: deployment missing entirely, no activity → row deleted
#[tokio::test]
async fn sweep_dead_deletes_agent_with_missing_deployment() {
    let app = app().await;
    let hash = format!("deployment_{}", uuid::Uuid::new_v4());
    // No deployment row at all

    let mut agent = Agent::new(hash);
    agent.last_heartbeat = Some(Utc::now() - chrono::Duration::days(60));
    insert_agent(&app.db_pool, &agent).await;

    let removed = db::agent::sweep_dead(&app.db_pool, 30).await.unwrap();
    assert_eq!(removed, 1);
    assert!(!agent_exists(&app.db_pool, agent.id).await);
}

/// Case 4: deployment alive, agent silent for a year → row preserved
#[tokio::test]
async fn sweep_dead_preserves_agent_with_live_deployment() {
    let app = app().await;
    let project_id = create_project(&app.db_pool, "test_user_id").await;
    let hash = format!("deployment_{}", uuid::Uuid::new_v4());
    create_deployment(&app.db_pool, project_id, "test_user_id", &hash, false).await;

    let mut agent = Agent::new(hash);
    agent.last_heartbeat = Some(Utc::now() - chrono::Duration::days(365));
    insert_agent(&app.db_pool, &agent).await;

    let removed = db::agent::sweep_dead(&app.db_pool, 30).await.unwrap();
    assert_eq!(removed, 0);
    assert!(agent_exists(&app.db_pool, agent.id).await);
}

/// Case 5: deployment_hash of length 86 (raw token) → row deleted
#[tokio::test]
async fn sweep_malformed_deletes_agent_with_86_char_hash() {
    let app = app().await;
    // 86-char base64url string — the pattern seen in production
    let bad_hash = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwx\
                    yz0123456789-_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef\
                    ghij";

    let agent = Agent::new(bad_hash.to_string());
    insert_agent(&app.db_pool, &agent).await;

    let removed = db::agent::sweep_malformed(&app.db_pool).await.unwrap();
    assert_eq!(removed, 1);
    assert!(!agent_exists(&app.db_pool, agent.id).await);
}

/// Case 6: empty deployment_hash → row deleted
#[tokio::test]
async fn sweep_malformed_deletes_agent_with_empty_hash() {
    let app = app().await;

    let agent = Agent::new(String::new());
    insert_agent(&app.db_pool, &agent).await;

    let removed = db::agent::sweep_malformed(&app.db_pool).await.unwrap();
    assert_eq!(removed, 1);
    assert!(!agent_exists(&app.db_pool, agent.id).await);
}

/// Valid hashes are not affected by sweep_malformed
#[tokio::test]
async fn sweep_malformed_preserves_valid_hash() {
    let app = app().await;
    let project_id = create_project(&app.db_pool, "test_user_id").await;
    let hash = format!("deployment_{}", uuid::Uuid::new_v4());
    create_deployment(&app.db_pool, project_id, "test_user_id", &hash, false).await;

    let agent = Agent::new(hash);
    insert_agent(&app.db_pool, &agent).await;

    let removed = db::agent::sweep_malformed(&app.db_pool).await.unwrap();
    assert_eq!(removed, 0);
    assert!(agent_exists(&app.db_pool, agent.id).await);
}

/// Case 7: after agent deletion, audit_log records survive and retain deployment_hash
#[tokio::test]
async fn audit_log_preserved_after_agent_deletion() {
    let app = app().await;
    let project_id = create_project(&app.db_pool, "test_user_id").await;
    let hash = format!("deployment_{}", uuid::Uuid::new_v4());
    create_deployment(&app.db_pool, project_id, "test_user_id", &hash, true).await;

    let mut agent = Agent::new(hash.clone());
    agent.last_heartbeat = Some(Utc::now() - chrono::Duration::days(60));
    insert_agent(&app.db_pool, &agent).await;

    let audit_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO audit_log (id, agent_id, deployment_hash, action, status, created_at)
         VALUES ($1, $2, $3, 'register', 'success', NOW() - interval '60 days')",
    )
    .bind(audit_id)
    .bind(agent.id)
    .bind(&hash)
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert audit log");

    // sweep_dead should NOT delete this agent (audit_log blocks it via the
    // retention window). But we want to test what happens when the agent IS
    // deleted — so delete it directly.
    db::agent::delete(&app.db_pool, agent.id).await.unwrap();

    // Audit log should survive, with agent_id NULLed and deployment_hash preserved
    let row = sqlx::query(
        "SELECT agent_id, deployment_hash FROM audit_log WHERE id = $1",
    )
    .bind(audit_id)
    .fetch_one(&app.db_pool)
    .await
    .expect("Audit log row should still exist");

    let agent_id: Option<uuid::Uuid> = row.get("agent_id");
    let dep_hash: Option<String> = row.get("deployment_hash");
    assert!(agent_id.is_none(), "agent_id should be NULL after ON DELETE SET NULL");
    assert_eq!(dep_hash.as_deref(), Some(hash.as_str()));
}

/// sweep_dead with no matching rows returns 0
#[tokio::test]
async fn sweep_dead_returns_zero_when_nothing_to_remove() {
    let app = app().await;
    let before = count_agents(&app.db_pool).await;
    let removed = db::agent::sweep_dead(&app.db_pool, 30).await.unwrap();
    assert_eq!(removed, 0);
    assert_eq!(count_agents(&app.db_pool).await, before);
}
