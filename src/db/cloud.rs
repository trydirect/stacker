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

    // If no name provided, we'll generate a default after insert (need the ID)
    let has_name = !cloud.name.is_empty();
    let insert_name = if has_name {
        cloud.name.clone()
    } else {
        // Temporary placeholder; will be updated below
        format!("{}-0", cloud.provider)
    };

    sqlx::query!(
        r#"
        INSERT INTO cloud (
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
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW() at time zone 'utc', NOW() at time zone 'utc')
        RETURNING id;
        "#,
        cloud.user_id,
        insert_name,
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
    .and_then(|mut cloud| {
        // Auto-generate name if not provided: "{provider}-{id}"
        if !has_name {
            cloud.name = format!("{}-{}", cloud.provider, cloud.id);
        }
        Ok(cloud)
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
            name=$3,
            provider=$4,
            cloud_token=$5,
            cloud_key=$6,
            cloud_secret=$7,
            save_token=$8,
            updated_at=NOW() at time zone 'utc'
        WHERE id = $1
        RETURNING *
        "#,
        cloud.id,
        cloud.user_id,
        cloud.name,
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
