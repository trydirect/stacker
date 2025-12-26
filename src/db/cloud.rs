use crate::models;
use sqlx::PgPool;
use tracing::Instrument;

pub async fn fetch(pool: &PgPool, id: i32) -> Result<Option<models::Cloud>, String> {
    tracing::info!("Fetch cloud {}", id);
    sqlx::query_as!(
        models::Cloud,
        r#"SELECT * FROM cloud WHERE id=$1 LIMIT 1 "#,
        id
    )
    .fetch_one(pool)
    .await
    .map(|cloud| Some(cloud))
    .or_else(|err| match err {
        sqlx::Error::RowNotFound => Ok(None),
        e => {
            tracing::error!("Failed to fetch cloud, error: {:?}", e);
            Err("Could not fetch data".to_string())
        }
    })
}

pub async fn fetch_by_user(pool: &PgPool, user_id: &str) -> Result<Vec<models::Cloud>, String> {
    let query_span = tracing::info_span!("Fetch clouds by user id.");
    sqlx::query_as!(
        models::Cloud,
        r#"
        SELECT
            *
        FROM cloud
        WHERE user_id=$1
        "#,
        user_id
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch cloud, error: {:?}", err);
        "".to_string()
    })
}

pub async fn insert(pool: &PgPool, mut cloud: models::Cloud) -> Result<models::Cloud, String> {
    let query_span = tracing::info_span!("Saving user's cloud data into the database");
    sqlx::query!(
        r#"
        INSERT INTO cloud (
        user_id,
        provider,
        cloud_token,
        cloud_key,
        cloud_secret,
        save_token,
        created_at,
        updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NOW() at time zone 'utc', NOW() at time zone 'utc')
        RETURNING id;
        "#,
        cloud.user_id,
        cloud.provider,
        cloud.cloud_token,
        cloud.cloud_key,
        cloud.cloud_secret,
        cloud.save_token
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(move |result| {
        cloud.id = result.id;
        cloud
    })
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        "Failed to insert".to_string()
    })
}

pub async fn update(pool: &PgPool, mut cloud: models::Cloud) -> Result<models::Cloud, String> {
    let query_span = tracing::info_span!("Updating user cloud");
    sqlx::query_as!(
        models::Cloud,
        r#"
        UPDATE cloud
        SET
            user_id=$2,
            provider=$3,
            cloud_token=$4,
            cloud_key=$5,
            cloud_secret=$6,
            save_token=$7,
            updated_at=NOW() at time zone 'utc'
        WHERE id = $1
        RETURNING *
        "#,
        cloud.id,
        cloud.user_id,
        cloud.provider,
        cloud.cloud_token,
        cloud.cloud_key,
        cloud.cloud_secret,
        cloud.save_token
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(|result| {
        tracing::info!("Cloud info {} have been saved", cloud.id);
        cloud.updated_at = result.updated_at;
        cloud
    })
    .map_err(|err| {
        tracing::error!("Failed to execute query: {:?}", err);
        "".to_string()
    })
}

#[tracing::instrument(name = "Delete cloud of a user.")]
pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, String> {
    tracing::info!("Delete cloud {}", id);
    sqlx::query::<sqlx::Postgres>("DELETE FROM cloud WHERE id = $1;")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| true)
        .map_err(|err| {
            tracing::error!("Failed to delete cloud: {:?}", err);
            "Failed to delete cloud".to_string()
        })
}
