use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::models::user::User;
use crate::models::RateCategory;
use sqlx::PgPool;
use tracing::Instrument;
use actix_web::{
    web,
    post,
    Responder, Result,
};

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user


#[tracing::instrument(name = "Add rating.")]
#[post("")]
pub async fn add_handler(
    user: web::ReqData<User>,
    form: web::Json<forms::Rating>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let query_span = tracing::info_span!("Check product existence by id.");
    match sqlx::query_as!(
        models::Product,
        r"SELECT * FROM product WHERE obj_id = $1",
        form.obj_id
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(product) => {
            tracing::info!("Found product: {:?}", product.obj_id);
        }
        Err(e) => {
            tracing::error!("Failed to fetch product: {:?}, error: {:?}", form.obj_id, e);
            return JsonResponse::<models::Rating>::build()
                .err(format!("Object not found {}", form.obj_id));
        }
    };

    let query_span = tracing::info_span!("Search for existing vote.");
    match sqlx::query!(
        r"SELECT id FROM rating where user_id=$1 AND obj_id=$2 AND category=$3 LIMIT 1",
        user.id,
        form.obj_id,
        form.category as RateCategory
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(record) => {
            tracing::info!(
                "rating exists: {:?}, user: {}, product: {}, category: {:?}",
                record.id,
                user.id,
                form.obj_id,
                form.category
            );
            return JsonResponse::build().conflict("Already rated".to_owned());
        }
        Err(sqlx::Error::RowNotFound) => {}
        Err(e) => {
            tracing::error!("Failed to fetch rating, error: {:?}", e);
            return JsonResponse::build().err(format!("Internal Server Error"));
        }
    }

    let query_span = tracing::info_span!("Saving new rating details into the database");
    // Insert rating
    match sqlx::query!(
        r#"
        INSERT INTO rating (user_id, obj_id, category, comment, hidden,rate,
        created_at,
        updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW() at time zone 'utc', NOW() at time zone 'utc')
        RETURNING id
        "#,
        user.id,
        form.obj_id,
        form.category as models::RateCategory,
        form.comment,
        false,
        form.rate
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(result) => {
            tracing::info!("New rating {} have been saved to database", result.id);

            JsonResponse::build()
                .set_id(result.id)
                .ok("Saved".to_owned())
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            JsonResponse::build().internal_error("Failed to insert".to_owned())
        }
    }
}
