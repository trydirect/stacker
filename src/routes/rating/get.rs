use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use tracing::Instrument;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[tracing::instrument(name = "Get rating.")]
#[get("/{id}")]
pub async fn get_handler(
    path: web::Path<(i32,)>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let rate_id = path.0;
    let query_span = tracing::info_span!("Search for rate id={}.", rate_id);
    match sqlx::query_as!(
        models::Rating,
        r"SELECT * FROM rating WHERE id=$1 LIMIT 1",
        rate_id
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(rating) => {
            tracing::info!("rating found: {:?}", rating.id);
            return JsonResponse::build().set_item(Some(rating)).ok("OK");
        }
        Err(sqlx::Error::RowNotFound) => {
            return JsonResponse::build().err("Not Found");
        }
        Err(e) => {
            tracing::error!("Failed to fetch rating, error: {:?}", e);
            return JsonResponse::build().err("Internal Server Error");
        }
    }
}

#[tracing::instrument(name = "Get all ratings.")]
#[get("")]
pub async fn default(path: web::Path<()>, pool: web::Data<PgPool>) -> Result<impl Responder> {
    let query_span = tracing::info_span!("Get all rates.");
    // let category = path.0;
    match sqlx::query_as!(models::Rating, r"SELECT * FROM rating")
        .fetch_all(pool.get_ref())
        .instrument(query_span)
        .await
    {
        Ok(rating) => {
            tracing::info!("Ratings found: {:?}", rating.len());
            return JsonResponse::build().set_list(rating).ok("OK".to_string());
        }
        Err(sqlx::Error::RowNotFound) => {
            return JsonResponse::build().err("Not Found");
        }
        Err(e) => {
            tracing::error!("Failed to fetch rating, error: {:?}", e);
            return JsonResponse::build().err("Internal Server Error");
        }
    }
}
