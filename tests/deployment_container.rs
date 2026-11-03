//! The observed-container table, against a real database.
//!
//! The upsert's behaviour is the whole point of the table — what a second
//! report does to an existing row, and what silence does not do — so it is
//! worth testing where the ON CONFLICT clause actually runs rather than
//! against a mock.

mod common;

use sqlx::PgPool;
use stacker::db;
use stacker::models::ObservedContainer;
use tokio::sync::OnceCell;

static APP: OnceCell<common::TestAppWithVaultShared> = OnceCell::const_new();

async fn pool() -> PgPool {
    common::get_or_init_vault_app_fresh(&APP)
        .await
        .expect("Failed to start test app")
        .db_pool
}

fn observed(name: &str, app_code: &str, scope: &str, state: &str) -> ObservedContainer {
    ObservedContainer {
        container_name: name.to_string(),
        app_code: Some(app_code.to_string()),
        scope: scope.to_string(),
        image: Some(format!("{app_code}:latest")),
        state: Some(state.to_string()),
    }
}

fn hash(suffix: &str) -> String {
    format!("deployment_test_{}_{}", suffix, uuid::Uuid::new_v4())
}

#[tokio::test]
async fn observing_a_container_records_it_once_and_then_updates_it() {
    let pool = pool().await;
    let deployment_hash = hash("upsert");

    db::deployment_container::record_observed(
        &pool,
        &deployment_hash,
        &[observed("project-app-1", "floci", "project", "running")],
    )
    .await
    .expect("first observation");

    let first = db::deployment_container::fetch_by_deployment(&pool, &deployment_hash)
        .await
        .expect("fetch");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].state.as_deref(), Some("running"));

    db::deployment_container::record_observed(
        &pool,
        &deployment_hash,
        &[observed("project-app-1", "floci", "project", "exited")],
    )
    .await
    .expect("second observation");

    let second = db::deployment_container::fetch_by_deployment(&pool, &deployment_hash)
        .await
        .expect("fetch");
    assert_eq!(second.len(), 1, "the same container is one row, not two");
    assert_eq!(second[0].state.as_deref(), Some("exited"));
    assert_eq!(
        second[0].first_seen_at, first[0].first_seen_at,
        "first_seen_at records when we met it, and does not move"
    );
    assert!(
        second[0].last_seen_at >= first[0].last_seen_at,
        "last_seen_at advances with each report"
    );
}

/// The requirement in one test: a report that omits a container does not
/// remove it. This is what a docker hiccup or a mid-recreate listing looks
/// like, and it used to empty the dashboard.
#[tokio::test]
async fn a_report_that_omits_a_container_leaves_it_in_place() {
    let pool = pool().await;
    let deployment_hash = hash("omission");

    db::deployment_container::record_observed(
        &pool,
        &deployment_hash,
        &[
            observed("project-app-1", "floci", "project", "running"),
            observed("statuspanel", "web", "platform", "running"),
        ],
    )
    .await
    .expect("first observation");

    db::deployment_container::record_observed(
        &pool,
        &deployment_hash,
        &[observed("project-app-1", "floci", "project", "running")],
    )
    .await
    .expect("partial observation");

    let rows = db::deployment_container::fetch_by_deployment(&pool, &deployment_hash)
        .await
        .expect("fetch");

    assert_eq!(rows.len(), 2, "the unmentioned container is still listed");
    let panel = rows
        .iter()
        .find(|row| row.container_name == "statuspanel")
        .expect("the container the second report said nothing about");
    assert_eq!(panel.scope, "platform");
    assert!(panel.removed_at.is_none());
}

#[tokio::test]
async fn removing_an_app_retires_its_containers() {
    let pool = pool().await;
    let deployment_hash = hash("removal");

    db::deployment_container::record_observed(
        &pool,
        &deployment_hash,
        &[
            observed("project-app-1", "floci", "project", "running"),
            observed("project-floci-ui-1", "floci-ui", "project", "running"),
        ],
    )
    .await
    .expect("observation");

    let retired =
        db::deployment_container::mark_removed_by_app_code(&pool, &deployment_hash, "floci-ui")
            .await
            .expect("mark removed");

    assert_eq!(retired, 1);
    let rows = db::deployment_container::fetch_by_deployment(&pool, &deployment_hash)
        .await
        .expect("fetch");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].container_name, "project-app-1");
}

/// Reinstalling brings the container back rather than leaving it retired: it is
/// demonstrably running again, so whatever removed it is out of date.
#[tokio::test]
async fn observing_a_retired_container_revives_it() {
    let pool = pool().await;
    let deployment_hash = hash("revival");

    db::deployment_container::record_observed(
        &pool,
        &deployment_hash,
        &[observed("project-app-1", "floci", "project", "running")],
    )
    .await
    .expect("observation");
    db::deployment_container::mark_removed_by_app_code(&pool, &deployment_hash, "floci")
        .await
        .expect("mark removed");
    assert!(
        db::deployment_container::fetch_by_deployment(&pool, &deployment_hash)
            .await
            .expect("fetch")
            .is_empty()
    );

    db::deployment_container::record_observed(
        &pool,
        &deployment_hash,
        &[observed("project-app-1", "floci", "project", "running")],
    )
    .await
    .expect("re-observation");

    let rows = db::deployment_container::fetch_by_deployment(&pool, &deployment_hash)
        .await
        .expect("fetch");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].removed_at.is_none());
}

#[tokio::test]
async fn containers_of_other_deployments_are_not_returned() {
    let pool = pool().await;
    let mine = hash("mine");
    let theirs = hash("theirs");

    db::deployment_container::record_observed(
        &pool,
        &mine,
        &[observed("project-app-1", "floci", "project", "running")],
    )
    .await
    .expect("observation");
    db::deployment_container::record_observed(
        &pool,
        &theirs,
        &[observed("project-app-1", "other", "project", "running")],
    )
    .await
    .expect("observation");

    let rows = db::deployment_container::fetch_by_deployment(&pool, &mine)
        .await
        .expect("fetch");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].app_code.as_deref(), Some("floci"));
}

/// The sweeper is housekeeping, not policy: a container retired moments ago
/// must survive it, or "removed" would become "forgotten" immediately.
#[tokio::test]
async fn the_sweeper_spares_recently_retired_rows() {
    let pool = pool().await;
    let deployment_hash = hash("sweep");

    db::deployment_container::record_observed(
        &pool,
        &deployment_hash,
        &[observed("project-app-1", "floci", "project", "running")],
    )
    .await
    .expect("observation");
    db::deployment_container::mark_removed_by_app_code(&pool, &deployment_hash, "floci")
        .await
        .expect("mark removed");

    db::deployment_container::sweep_removed(&pool, 30)
        .await
        .expect("sweep");

    let survived = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM deployment_container WHERE deployment_hash = $1",
        deployment_hash
    )
    .fetch_one(&pool)
    .await
    .expect("count");

    assert_eq!(survived, Some(1));
}
