//! Runtime validation of the sandbox db layer (WS-S S1).

mod common;

use stacker::db;
use uuid::Uuid;

#[tokio::test]
async fn sandbox_insert_fetch_status_and_expiry() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let pool = &app.db_pool;
    let project_id = common::create_test_project(pool, common::USER_A_ID).await;

    let sandbox = db::sandbox::insert(
        pool,
        common::USER_A_ID,
        Some(project_id),
        None,
        "alpine:3.20",
        "runc",
        "isolated",
        0.5,
        256,
        64,
        1800,
        86400,
        None,
    )
    .await
    .expect("insert sandbox");
    assert_eq!(sandbox.status, "creating");
    assert_eq!(sandbox.image, "alpine:3.20");
    assert_eq!(sandbox.network_mode, "isolated");

    // Fetch by id.
    let fetched = db::sandbox::fetch(pool, sandbox.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, sandbox.id);

    // Status transition.
    db::sandbox::update_status(pool, sandbox.id, "running")
        .await
        .expect("running");
    let running = db::sandbox::fetch(pool, sandbox.id).await.unwrap().unwrap();
    assert_eq!(running.status, "running");

    // Listing by user.
    assert_eq!(
        db::sandbox::list_by_user(pool, common::USER_A_ID)
            .await
            .unwrap()
            .len(),
        1
    );

    // Not reapable yet (expires_at is in the future, just created).
    let reapable = db::sandbox::list_reapable(pool).await.unwrap();
    assert!(reapable.is_empty());

    // Cleanup.
    db::sandbox::delete(pool, sandbox.id).await.expect("delete");
    assert!(db::sandbox::fetch(pool, sandbox.id)
        .await
        .unwrap()
        .is_none());
    let _ = Uuid::new_v4();
}
