use crate::models;
use actix_web::{get, web, Responder, Result};
use serde_derive::Serialize;
use sqlx::PgPool;
use tracing::Instrument;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[derive(Serialize)]
struct JsonResponse {
    status: String,
    message: String,
    code: u32,
    rating: Option<models::Rating>,
    objects: Option<Vec<models::Rating>>,
}

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
            tracing::info!("rating found: {:?}", rating.id,);
            return Ok(web::Json(JsonResponse {
                status: "Success".to_string(),
                code: 200,
                message: "".to_string(),
                rating: Some(rating),
                objects: None
            }));
        }
        Err(sqlx::Error::RowNotFound) => {
            return Ok(web::Json(JsonResponse {
                status: "Error".to_string(),
                code: 404,
                message: format!("Not Found"),
                rating: None,
                objects: None
            }));
        }
        Err(e) => {
            tracing::error!("Failed to fetch rating, error: {:?}", e);
            return Ok(web::Json(JsonResponse {
                status: "Error".to_string(),
                code: 500,
                message: format!("Internal Server Error"),
                rating: None,
                objects: None
            }));
        }
    }
}

#[tracing::instrument(name = "Get all ratings.")]
#[get("")]
pub async fn list_handler(
    path: web::Path<()>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {

    let query_span = tracing::info_span!("Get all rates.");
    // let category = path.0;
    match sqlx::query_as!(
        models::Rating,
        r"SELECT * FROM rating"
    )
        .fetch_all(pool.get_ref())
        .instrument(query_span)
        .await
    {
        Ok(rating) => {
            tracing::info!("Ratings found: {:?}", rating.len());
            return Ok(web::Json(JsonResponse {
                status: "Success".to_string(),
                code: 200,
                message: "".to_string(),
                rating: None,
                objects: Some(rating),
            }));
        }
        Err(sqlx::Error::RowNotFound) => {
            return Ok(web::Json(JsonResponse {
                status: "Error".to_string(),
                code: 404,
                message: format!("Not Found"),
                rating: None,
                objects: None
            }));
        }
        Err(e) => {
            tracing::error!("Failed to fetch rating, error: {:?}", e);
            return Ok(web::Json(JsonResponse {
                status: "Error".to_string(),
                code: 500,
                message: format!("Internal Server Error"),
                rating: None,
                objects: None
            }));
        }
    }
}
