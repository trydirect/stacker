use crate::models;
use sqlx::{PgPool, Row};
use tracing::Instrument;

pub fn visible_average_subquery_for_obj_id(obj_id_expr: &str) -> String {
    format!(
        "(SELECT AVG(r.rate)::float8 FROM rating r WHERE r.obj_id = {obj_id_expr} AND r.rate IS NOT NULL AND r.hidden = false)"
    )
}

pub fn visible_average_subquery_for_creator(creator_user_id_expr: &str) -> String {
    format!(
        "(SELECT AVG(r.rate)::float8 FROM stack_template t JOIN rating r ON r.obj_id = t.product_id WHERE t.creator_user_id = {creator_user_id_expr} AND t.status = 'approved' AND r.rate IS NOT NULL AND r.hidden = false)"
    )
}

pub async fn fetch_all(pool: &PgPool) -> Result<Vec<models::Rating>, String> {
    let query_span = tracing::info_span!("Fetch all ratings.");
    sqlx::query_as::<_, models::Rating>(
        r#"SELECT
            id,
            user_id,
            obj_id,
            category,
            comment,
            hidden,
            rate,
            created_at,
            updated_at
        FROM rating
        ORDER BY id DESC
        "#,
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute fetch query: {:?}", e);
        "".to_string()
    })
}

pub async fn fetch(pool: &PgPool, id: i32) -> Result<Option<models::Rating>, String> {
    let query_span = tracing::info_span!("Fetch rating by id");
    sqlx::query_as::<_, models::Rating>(
        r#"SELECT
            id,
            user_id,
            obj_id,
            category,
            comment,
            hidden,
            rate,
            created_at,
            updated_at
        FROM rating
        WHERE id=$1
        LIMIT 1"#,
    )
    .bind(id)
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(|rating| Some(rating))
    .or_else(|e| match e {
        sqlx::Error::RowNotFound => Ok(None),
        s => {
            tracing::error!("Failed to execute fetch query: {:?}", s);
            Err("".to_string())
        }
    })
}

pub async fn fetch_by_obj_and_user_and_category(
    pool: &PgPool,
    obj_id: i32,
    user_id: String,
    category: models::RateCategory,
) -> Result<Option<models::Rating>, String> {
    let query_span = tracing::info_span!("Fetch rating by obj, user and category.");
    sqlx::query_as::<_, models::Rating>(
        r#"SELECT
            id,
            user_id,
            obj_id,
            category,
            comment,
            hidden,
            rate,
            created_at,
            updated_at
        FROM rating
        WHERE user_id=$1
            AND obj_id=$2
            AND category=$3
        LIMIT 1"#,
    )
    .bind(user_id)
    .bind(obj_id)
    .bind(category)
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(|rating| Some(rating))
    .or_else(|e| match e {
        sqlx::Error::RowNotFound => Ok(None),
        s => {
            tracing::error!("Failed to execute fetch query: {:?}", s);
            Err("".to_string())
        }
    })
}

pub async fn insert(pool: &PgPool, mut rating: models::Rating) -> Result<models::Rating, String> {
    let query_span = tracing::info_span!("Saving new rating details into the database");
    sqlx::query(
        r#"
        INSERT INTO rating (user_id, obj_id, category, comment, hidden, rate, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW() at time zone 'utc', NOW() at time zone 'utc')
        RETURNING id
        "#,
    )
    .bind(&rating.user_id)
    .bind(rating.obj_id)
    .bind(rating.category)
    .bind(&rating.comment)
    .bind(rating.hidden)
    .bind(rating.rate)
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(move |result| {
        rating.id = result.get("id");
        rating
    })
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        "Failed to insert".to_string()
    })
}

pub async fn update(pool: &PgPool, rating: models::Rating) -> Result<models::Rating, String> {
    let query_span = tracing::info_span!("Updating rating into the database");
    sqlx::query(
        r#"
        UPDATE rating
        SET
            comment=$1,
            rate=$2,
            hidden=$3,
            updated_at=NOW() at time zone 'utc'
        WHERE id = $4
        "#,
    )
    .bind(&rating.comment)
    .bind(rating.rate)
    .bind(rating.hidden)
    .bind(rating.id)
    .execute(pool)
    .instrument(query_span)
    .await
    .map(|_| {
        tracing::info!("Rating {} has been saved to the database", rating.id);
        rating
    })
    .map_err(|err| {
        tracing::error!("Failed to execute query: {:?}", err);
        "".to_string()
    })
}

pub async fn fetch_all_visible(pool: &PgPool) -> Result<Vec<models::Rating>, String> {
    let query_span = tracing::info_span!("Fetch all ratings.");
    sqlx::query_as::<_, models::Rating>(
        r#"SELECT
            id,
            user_id,
            obj_id,
            category,
            comment,
            hidden,
            rate,
            created_at,
            updated_at
        FROM rating
        WHERE hidden = false
        ORDER BY id DESC
        "#,
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute fetch query: {:?}", e);
        "".to_string()
    })
}

pub async fn delete(pool: &PgPool, rating: models::Rating) -> Result<(), String> {
    let query_span = tracing::info_span!("Deleting rating from the database");
    sqlx::query(
        r#"
        DELETE FROM rating
        WHERE id = $1
        "#,
    )
    .bind(rating.id)
    .execute(pool)
    .instrument(query_span)
    .await
    .map(|_| {
        tracing::info!("Rating {} has been deleted from the database", rating.id);
    })
    .map_err(|err| {
        tracing::error!("Failed to execute query: {:?}", err);
        "".to_string()
    })
}
