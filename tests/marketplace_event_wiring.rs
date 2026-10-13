//! Regression: the view/deploy counter endpoints must also write event rows,
//! otherwise `marketplace_event` / `marketplace_template_event` stay empty and
//! creator analytics always reads zero.
mod common;

use sqlx::Row;
use uuid::Uuid;

async fn insert_template(pool: &sqlx::PgPool, id: Uuid) {
    sqlx::query(
        r#"INSERT INTO stack_template (id, creator_user_id, creator_name, name, slug,
            short_description, status, tags, tech_stack, view_count, deploy_count, created_at, updated_at)
           VALUES ($1, 'vendor-x', 'Vendor X', $2, $3, 'd', 'approved', '[]'::jsonb, '{}'::jsonb, 0, 0, NOW(), NOW())"#,
    )
    .bind(id)
    .bind(format!("T {id}"))
    .bind(format!("t-{id}"))
    .execute(pool)
    .await
    .expect("insert template");
}

async fn count(pool: &sqlx::PgPool, table: &str, id: Uuid, kind: &str) -> i64 {
    sqlx::query(&format!(
        "SELECT COUNT(*)::bigint AS c FROM {table} WHERE template_id = $1 AND event_type = $2"
    ))
    .bind(id)
    .bind(kind)
    .fetch_one(pool)
    .await
    .expect("count")
    .get("c")
}

#[tokio::test]
async fn detail_view_records_events_but_deploy_counter_does_not() {
    let Some(app) = common::spawn_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };
    let id = Uuid::new_v4();
    insert_template(&app.db_pool, id).await;

    let resp = reqwest::get(format!("{}/api/templates/t-{id}", app.address))
        .await
        .expect("request");
    assert!(resp.status().is_success(), "detail: {}", resp.status());
    assert_eq!(count(&app.db_pool, "marketplace_template_event", id, "view").await, 1);
    assert_eq!(count(&app.db_pool, "marketplace_event", id, "view").await, 1);

    stacker::db::marketplace::increment_deploy_count(&app.db_pool, &id)
        .await
        .expect("deploy count");
    // The unauthenticated counter must not forge deploy analytics.
    assert_eq!(count(&app.db_pool, "marketplace_template_event", id, "deploy").await, 0);
    assert_eq!(count(&app.db_pool, "marketplace_event", id, "deploy").await, 0);
}

#[tokio::test]
async fn unknown_template_records_no_event() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let id = Uuid::new_v4();
    let _ = stacker::db::marketplace::increment_view_count(&app.db_pool, &id).await;
    assert_eq!(count(&app.db_pool, "marketplace_template_event", id, "view").await, 0);
    assert_eq!(count(&app.db_pool, "marketplace_event", id, "view").await, 0);
}

#[tokio::test]
async fn insert_helpers_write_creator_analytics_source() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let id = Uuid::new_v4();
    insert_template(&app.db_pool, id).await;
    stacker::db::marketplace::insert_view_event(&app.db_pool, id, "u1", serde_json::json!({}))
        .await
        .expect("view");
    stacker::db::marketplace::insert_deploy_event(&app.db_pool, id, "u2", "hetzner", serde_json::json!({}))
        .await
        .expect("deploy");
    let a = stacker::db::marketplace::get_vendor_analytics(&app.db_pool, "vendor-x", Some("all"))
        .await
        .expect("analytics");
    let t = a.templates.iter().find(|t| t.template_id == id).expect("template in analytics");
    assert_eq!((t.views, t.deployments), (1, 1));
}
