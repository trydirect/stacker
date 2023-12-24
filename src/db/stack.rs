use crate::models;
use sqlx::PgPool;
use tracing::Instrument;

pub async fn fetch(pool: &PgPool, id: i32) -> Result<Option<models::Stack>, String> {
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
    .map(|stack| Some(stack))
    .or_else(|err| match err {
        sqlx::Error::RowNotFound => Ok(None),
        e => {
            tracing::error!("Failed to fetch stack, error: {:?}", e);
            Err("Could not fetch data".to_string())
        }
    })
}

pub async fn fetch_by_user(pool: &PgPool, user_id: &str) -> Result<Vec<models::Stack>, String> {
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

pub async fn fetch_one_by_name(pool: &PgPool, name: &str) -> Result<Option<models::Stack>, String> {
    let query_span = tracing::info_span!("Fetch one stack by name.");
    sqlx::query_as!(
        models::Stack,
        r#"
        SELECT
            *
        FROM user_stack
        WHERE name=$1
        LIMIT 1
        "#,
        name
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(|stack| Some(stack))
    .or_else(|err| match err {
        sqlx::Error::RowNotFound => Ok(None),
        err => {
            tracing::error!("Failed to fetch one stack by name, error: {:?}", err);
            Err("".to_string())
        }
    })
}

pub async fn insert(pool: &PgPool, mut stack: models::Stack) -> Result<models::Stack, String> {
    let query_span = tracing::info_span!("Saving new stack into the database");
    sqlx::query!(
        r#"
        INSERT INTO user_stack (stack_id, user_id, name, body, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id;
        "#,
        stack.stack_id,
        stack.user_id,
        stack.name,
        stack.body,
        stack.created_at,
        stack.updated_at,
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(move |result| {
        stack.id = result.id;
        stack
    })
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        "Failed to insert".to_string()
    })
}
