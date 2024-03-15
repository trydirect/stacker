use crate::models;
use sqlx::PgPool;
use tracing::Instrument;

pub async fn insert(pool: &PgPool, mut deployment: models::Deployment) -> Result<models::Deployment, String> {
    let query_span = tracing::info_span!("Saving new deployment into the database");
    sqlx::query!(
        r#"
        INSERT INTO deployment (project_id, deleted, status, body, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id;
        "#,
        deployment.project_id,
        deployment.deleted,
        deployment.status,
        deployment.body,
        deployment.created_at,
        deployment.updated_at,
    )
        .fetch_one(pool)
        .instrument(query_span)
        .await
        .map(move |result| {
            deployment.id = result.id;
            deployment
        })
        .map_err(|e| {
            tracing::error!("Failed to execute query: {:?}", e);
            "Failed to insert".to_string()
        })
}

pub async fn update(pool: &PgPool, mut deployment: models::Deployment) -> Result<models::Deployment, String> {
    let query_span = tracing::info_span!("Updating user deployment into the database");
    sqlx::query_as!(
        models::Deployment,
        r#"
        UPDATE deployment
        SET
            project_id=$2,
            deleted=$3,
            status=$4,
            body=$5,
            updated_at=NOW() at time zone 'utc'
        WHERE id = $1
        RETURNING *
        "#,
        deployment.id,
        deployment.project_id,
        deployment.deleted,
        deployment.status,
        deployment.body,
    )
        .fetch_one(pool)
        .instrument(query_span)
        .await
        .map(|result|{
            tracing::info!("Deployment {} has been updated", deployment.id);
            deployment.updated_at = result.updated_at;
            deployment
        })
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            "".to_string()
        })
}
