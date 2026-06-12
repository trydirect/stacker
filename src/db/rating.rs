use crate::models;
use sqlx::{PgPool, Row};
use tracing::Instrument;

pub fn visible_average_subquery_for_obj_id(obj_id_expr: &str) -> String {
    format!(
        "(SELECT AVG(r.rate)::float8 / 2.0 FROM rating r WHERE r.obj_id = {obj_id_expr} AND r.category = 'application' AND r.rate IS NOT NULL AND r.hidden = false)"
    )
}

pub fn visible_count_subquery_for_obj_id(obj_id_expr: &str) -> String {
    format!(
        "(SELECT COUNT(*)::bigint FROM rating r WHERE r.obj_id = {obj_id_expr} AND r.category = 'application' AND r.rate IS NOT NULL AND r.hidden = false)"
    )
}

pub fn visible_average_subquery_for_creator(creator_user_id_expr: &str) -> String {
    format!(
        "(SELECT AVG(r.rate)::float8 / 2.0 FROM stack_template t JOIN rating r ON r.obj_id = t.product_id WHERE t.creator_user_id = {creator_user_id_expr} AND t.status = 'approved' AND r.category = 'application' AND r.rate IS NOT NULL AND r.hidden = false)"
    )
}

pub fn visible_count_subquery_for_creator(creator_user_id_expr: &str) -> String {
    format!(
        "(SELECT COUNT(*)::bigint FROM stack_template t JOIN rating r ON r.obj_id = t.product_id WHERE t.creator_user_id = {creator_user_id_expr} AND t.status = 'approved' AND r.category = 'application' AND r.rate IS NOT NULL AND r.hidden = false)"
    )
}

async fn fetch_approved_template_product_id(
    pool: &PgPool,
    template_id: &uuid::Uuid,
) -> Result<Option<i32>, String> {
    sqlx::query(
        r#"WITH selected AS (
            SELECT id, COALESCE(product_id, hashtext(id::text)) AS product_id
            FROM stack_template
            WHERE id = $1 AND status = 'approved'
            LIMIT 1
        ), inserted_product AS (
            INSERT INTO product (id, obj_id, obj_type, created_at, updated_at)
            SELECT product_id, product_id, 'marketplace_template', NOW(), NOW()
            FROM selected
            ON CONFLICT (id) DO NOTHING
        )
        UPDATE stack_template t
        SET product_id = selected.product_id
        FROM selected
        WHERE t.id = selected.id
        RETURNING t.product_id"#,
    )
    .bind(template_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.and_then(|row| row.get::<Option<i32>, _>("product_id")))
    .map_err(|err| {
        tracing::error!("Failed to fetch approved template product_id: {:?}", err);
        "Internal Server Error".to_string()
    })
}

pub async fn template_summary(
    pool: &PgPool,
    template_id: &uuid::Uuid,
) -> Result<Option<models::TemplateRatingSummary>, String> {
    let Some(product_id) = fetch_approved_template_product_id(pool, template_id).await? else {
        return Ok(None);
    };

    let row = sqlx::query(
        r#"SELECT
            AVG(rate)::float8 / 2.0 AS rating,
            COUNT(*)::bigint AS rating_count
        FROM rating
        WHERE obj_id = $1
          AND category = 'application'
          AND rate IS NOT NULL
          AND hidden = false"#,
    )
    .bind(product_id)
    .fetch_one(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch template rating summary: {:?}", err);
        "Internal Server Error".to_string()
    })?;

    Ok(Some(models::TemplateRatingSummary {
        template_id: *template_id,
        rating: row.get("rating"),
        rating_count: row.get("rating_count"),
        rating_scale: 5,
    }))
}

pub async fn fetch_template_rating_for_user(
    pool: &PgPool,
    template_id: &uuid::Uuid,
    user_id: &str,
) -> Result<Option<models::MyTemplateRating>, String> {
    let Some(product_id) = fetch_approved_template_product_id(pool, template_id).await? else {
        return Ok(None);
    };

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
        WHERE obj_id = $1
          AND user_id = $2
          AND category = 'application'
          AND hidden = false
        LIMIT 1"#,
    )
    .bind(product_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map(|rating| {
        rating.map(|rating| models::MyTemplateRating {
            template_id: *template_id,
            rating_id: rating.id,
            rating: rating.rate.unwrap_or_default() as f64 / 2.0,
            rating_scale: 5,
            comment: rating.comment,
            created_at: rating.created_at,
            updated_at: rating.updated_at,
        })
    })
    .map_err(|err| {
        tracing::error!("Failed to fetch template rating for user: {:?}", err);
        "Internal Server Error".to_string()
    })
}

pub async fn upsert_template_rating_for_user(
    pool: &PgPool,
    template_id: &uuid::Uuid,
    user_id: &str,
    stars: i32,
    comment: Option<String>,
) -> Result<Option<models::MyTemplateRating>, String> {
    let Some(product_id) = fetch_approved_template_product_id(pool, template_id).await? else {
        return Ok(None);
    };
    let internal_rate = stars * 2;

    let existing = fetch_by_obj_and_user_and_category(
        pool,
        product_id,
        user_id.to_string(),
        models::RateCategory::Application,
    )
    .await?;

    let rating = if let Some(mut rating) = existing {
        rating.rate = Some(internal_rate);
        rating.comment = comment;
        rating.hidden = Some(false);
        update(pool, rating).await?
    } else {
        let mut rating = models::Rating::default();
        rating.user_id = user_id.to_string();
        rating.obj_id = product_id;
        rating.category = models::RateCategory::Application;
        rating.comment = comment;
        rating.hidden = Some(false);
        rating.rate = Some(internal_rate);
        insert(pool, rating).await?
    };

    Ok(Some(models::MyTemplateRating {
        template_id: *template_id,
        rating_id: rating.id,
        rating: rating.rate.unwrap_or_default() as f64 / 2.0,
        rating_scale: 5,
        comment: rating.comment,
        created_at: rating.created_at,
        updated_at: rating.updated_at,
    }))
}

pub async fn hide_template_rating_for_user(
    pool: &PgPool,
    template_id: &uuid::Uuid,
    user_id: &str,
) -> Result<Option<()>, String> {
    let Some(product_id) = fetch_approved_template_product_id(pool, template_id).await? else {
        return Ok(None);
    };

    let Some(mut rating) = fetch_by_obj_and_user_and_category(
        pool,
        product_id,
        user_id.to_string(),
        models::RateCategory::Application,
    )
    .await?
    else {
        return Ok(None);
    };

    if rating.hidden == Some(true) {
        return Ok(None);
    }

    rating.hidden = Some(true);
    update(pool, rating).await?;
    Ok(Some(()))
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
