mod common;

use sqlx::Row;
use stacker::{db, models};
use uuid::Uuid;

async fn create_test_template(pool: &sqlx::PgPool, user_id: &str) -> Uuid {
    let slug = format!("rollback-template-{}", Uuid::new_v4());
    let name = format!("Rollback Template {}", Uuid::new_v4());

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
    .bind(name)
    .bind(slug)
    .fetch_one(pool)
    .await
    .expect("template insert should succeed")
    .get::<Uuid, _>("id")
}

#[tokio::test]
async fn project_insert_persists_source_template_id_and_template_version() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let template_id = create_test_template(&app.db_pool, common::USER_A_ID).await;
    let project = models::Project {
        id: 0,
        stack_id: Uuid::new_v4(),
        user_id: common::USER_A_ID.to_string(),
        name: "marketplace-project".to_string(),
        metadata: serde_json::json!({}),
        request_json: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        source_template_id: Some(template_id),
        template_version: Some("2.4.1".to_string()),
    };

    let inserted = db::project::insert(&app.db_pool, project)
        .await
        .expect("project insert should succeed");
    let fetched = db::project::fetch(&app.db_pool, inserted.id)
        .await
        .expect("project fetch should succeed")
        .expect("project should exist");

    assert_eq!(fetched.source_template_id, Some(template_id));
    assert_eq!(fetched.template_version.as_deref(), Some("2.4.1"));
}

#[tokio::test]
async fn project_update_persists_source_template_id_and_template_version() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let template_id = create_test_template(&app.db_pool, common::USER_A_ID).await;

    let mut project = db::project::fetch(&app.db_pool, project_id)
        .await
        .expect("project fetch should succeed")
        .expect("project should exist");
    project.source_template_id = Some(template_id);
    project.template_version = Some("3.0.0".to_string());

    db::project::update(&app.db_pool, project)
        .await
        .expect("project update should succeed");

    let fetched = db::project::fetch(&app.db_pool, project_id)
        .await
        .expect("project fetch should succeed")
        .expect("project should exist");

    assert_eq!(fetched.source_template_id, Some(template_id));
    assert_eq!(fetched.template_version.as_deref(), Some("3.0.0"));
}
