use sqlx::PgPool;
use crate::models;
use tracing::Instrument;

pub async fn fetch_by_obj_and_user_and_category(pool: &PgPool, obj_id: i32, user_id: String, category: models::RateCategory) -> Result<models::Rating, String> {
    let query_span = tracing::info_span!("Search for existing vote.");
    sqlx::query_as!(
        models::Rating,
        r"SELECT * FROM rating where user_id=$1 AND obj_id=$2 AND category=$3 LIMIT 1",
        user_id,
        obj_id,
        "ok" //todo put there the category
        //category.into() //todo
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        match e {
            sqlx::Error::RowNotFound => "client not found".to_string(),
            s => {
                tracing::error!("Failed to execute fetch query: {:?}", s);
                "".to_string()
            }
        }
    })
}

pub async fn insert(pool: &PgPool, mut rating: models::Rating) -> Result<models::Rating, String> {
    let query_span = tracing::info_span!("Saving new rating details into the database");
    sqlx::query!(
        r#"
        INSERT INTO rating (user_id, obj_id, category, comment, hidden, rate, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW() at time zone 'utc', NOW() at time zone 'utc')
        RETURNING id
        "#,
        rating.user_id,
        rating.obj_id,
        rating.category,
        rating.comment,
        rating.hidden,
        rating.rate
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(move |result| {
        rating.id = result.id;
        rating
    })
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        "Failed to insert".to_string()
    })
}
