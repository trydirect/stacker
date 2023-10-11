use crate::models;
use actix_web::get;
use actix_web::{web, Responder, Result};
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
    //id: Option<i32>,
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
            /*
            tracing::info!(
                "rating exists: {:?}, user: {}, product: {}, category: {:?}",
                record.id,
                user.id,
                form.obj_id,
                form.category
            );
            */
            let rating_json = serde_json::ser::to_string(&rating).unwrap();
            return Ok(web::Json(JsonResponse {
                status: "Error".to_string(),
                code: 409,
                message: rating_json,
                //id: Some(record.id),
            }));
        }
        Err(sqlx::Error::RowNotFound) => {
            return Ok(web::Json(JsonResponse {
                status: "Error".to_string(),
                code: 404,
                message: format!("Not Found"),
            }));
        }
        Err(e) => {
            tracing::error!("Failed to fetch rating, error: {:?}", e);
            return Ok(web::Json(JsonResponse {
                status: "Error".to_string(),
                code: 500,
                message: format!("Internal Server Error"),
            }));
        }
    }
}
