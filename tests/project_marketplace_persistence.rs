mod common;

use serde_json::json;
use sqlx::Row;
use stacker::{db, models};
use uuid::Uuid;

async fn insert_template(pool: &sqlx::PgPool, user_id: &str) -> Uuid {
    sqlx::query(
        r#"INSERT INTO stack_template (
            creator_user_id,
            name,
            slug,
            status,
            tags,
            tech_stack,
            infrastructure_requirements
        )
        VALUES ($1, $2, $3, 'approved', '[]'::jsonb, '{}'::jsonb, '{}'::jsonb)
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(format!("Template {}", Uuid::new_v4()))
    .bind(format!("template-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("Failed to insert template")
    .get::<Uuid, _>("id")
}

#[tokio::test]
async fn project_insert_preserves_marketplace_provenance() {
    let Some(app) = common::spawn_app().await else {
        return;
    };

    let template_id = insert_template(&app.db_pool, common::USER_A_ID).await;
    let mut project = models::Project::new(
        common::USER_A_ID.to_string(),
        "rollback-project".to_string(),
        json!({"custom": {"web": [], "custom_stack_code": "rollback-project"}}),
        json!({"custom": {"web": [], "custom_stack_code": "rollback-project"}}),
    );
    project.source_template_id = Some(template_id);
    project.template_version = Some("1.0.0".to_string());

    let saved = db::project::insert(&app.db_pool, project)
        .await
        .expect("project insert should succeed");
    let fetched = db::project::fetch(&app.db_pool, saved.id)
        .await
        .expect("project fetch should succeed")
        .expect("project should exist");

    assert_eq!(fetched.source_template_id, Some(template_id));
    assert_eq!(fetched.template_version.as_deref(), Some("1.0.0"));
}

#[tokio::test]
async fn project_update_preserves_marketplace_provenance() {
    let Some(app) = common::spawn_app().await else {
        return;
    };

    let template_id = insert_template(&app.db_pool, common::USER_A_ID).await;
    let mut project = models::Project::new(
        common::USER_A_ID.to_string(),
        "rollback-project".to_string(),
        json!({"custom": {"web": [], "custom_stack_code": "rollback-project"}}),
        json!({"custom": {"web": [], "custom_stack_code": "rollback-project"}}),
    );
    project.source_template_id = Some(template_id);
    project.template_version = Some("1.0.0".to_string());

    let mut saved = db::project::insert(&app.db_pool, project)
        .await
        .expect("project insert should succeed");
    saved.metadata = json!({"custom": {"web": [], "custom_stack_code": "rollback-updated"}});
    saved.request_json = saved.metadata.clone();
    saved.template_version = Some("2.0.0".to_string());
    let saved_id = saved.id;

    db::project::update(&app.db_pool, saved)
        .await
        .expect("project update should succeed");

    let fetched = db::project::fetch(&app.db_pool, saved_id)
        .await
        .expect("project fetch should succeed")
        .expect("project should exist");

    assert_eq!(fetched.source_template_id, Some(template_id));
    assert_eq!(fetched.template_version.as_deref(), Some("2.0.0"));
}
