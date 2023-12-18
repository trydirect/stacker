use sqlx::PgPool;
use crate::models;
use tracing::Instrument;

pub async fn fetch(pool: &PgPool, id: i32) -> Result<models::Stack, String> {
    tracing::info!("Fecth stack {}", id);
    sqlx::query_as!(
        models::Stack,
        r#"
        SELECT
            *
        FROM user_stack
        WHERE id=$1
        LIMIT 1
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .map_err(|err| {
        match err {
            sqlx::Error::RowNotFound => "".to_string(),
            e => {
                tracing::error!("Failed to fetch stack, error: {:?}", e);
                return "Could not fetch data".to_string();
            }

        }
    })
}

pub async fn fetch_by_user(pool: &PgPool, user_id: String) -> Result<Vec<models::Stack>, String> {
    let query_span = tracing::info_span!("Fetch stacks by user id.");
    sqlx::query_as!(
        models::Stack,
        r#"
        SELECT
            *
        FROM user_stack
        WHERE user_id=$1
        "#,
        user_id
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch stack, error: {:?}", err);
        "".to_string()
    })
}
