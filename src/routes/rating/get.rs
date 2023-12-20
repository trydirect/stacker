use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
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
    let rating = db::rating::fetch(pool.get_ref(), rate_id)
        .await
        .map_err(|_err| JsonResponse::<models::Rating>::build().internal_server_error(""))?
        .ok_or_else(|| JsonResponse::<models::Rating>::build().not_found("not found"))?;

    Ok(JsonResponse::build().set_item(rating).ok("OK"))
}

#[tracing::instrument(name = "Get all ratings.")]
#[get("")]
pub async fn list_handler(path: web::Path<()>, pool: web::Data<PgPool>) -> Result<impl Responder> {
    db::rating::fetch_all(pool.get_ref())
        .await
        .map(|ratings| JsonResponse::build().set_list(ratings).ok("OK"))
        .map_err(|err| JsonResponse::<models::Rating>::build().internal_server_error(""))
}
