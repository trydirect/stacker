use crate::models;
use sqlx::PgPool;
use tracing::Instrument;

pub async fn upsert(
    pool: &PgPool,
    project_id: i32,
    user_id: &str,
    role: &str,
    created_by: &str,
) -> Result<models::ProjectMember, String> {
    let query_span = tracing::info_span!("Upsert project member", project_id, user_id);
    sqlx::query_as::<_, models::ProjectMember>(
        r#"
        INSERT INTO project_member (project_id, user_id, role, created_by, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW() at time zone 'utc', NOW() at time zone 'utc')
        ON CONFLICT (project_id, user_id)
        DO UPDATE SET
            role = EXCLUDED.role,
            created_by = EXCLUDED.created_by,
            updated_at = NOW() at time zone 'utc'
        RETURNING project_id, user_id, role, created_by, created_at, updated_at
        "#,
    )
    .bind(project_id)
    .bind(user_id)
    .bind(role)
    .bind(created_by)
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to upsert project member: {:?}", err);
        "Failed to save project member".to_string()
    })
}

pub async fn fetch(
    pool: &PgPool,
    project_id: i32,
    user_id: &str,
) -> Result<Option<models::ProjectMember>, String> {
    let query_span = tracing::info_span!("Fetch project member", project_id, user_id);
    sqlx::query_as::<_, models::ProjectMember>(
        r#"
        SELECT project_id, user_id, role, created_by, created_at, updated_at
        FROM project_member
        WHERE project_id = $1 AND user_id = $2
        LIMIT 1
        "#,
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch project member: {:?}", err);
        "Failed to fetch project member".to_string()
    })
}
