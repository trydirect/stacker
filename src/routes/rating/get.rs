use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;

#[tracing::instrument(name = "Anonymouse get rating.")]
#[get("/{id}")]
pub async fn anonymous_get_handler(
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let rate_id = path.0;
    let rating = db::rating::fetch(pg_pool.get_ref(), rate_id)
        .await
        .map_err(|_err| JsonResponse::<models::Rating>::build().internal_server_error(""))
        .and_then(|rating| {
            match rating {
                Some(rating) if rating.hidden == Some(false) => { Ok(rating) },
                _ => Err(JsonResponse::<models::Rating>::build().not_found("not found"))
            }
        })?;

    Ok(JsonResponse::build().set_item(rating).ok("OK"))
}

#[tracing::instrument(name = "Anonymous get all ratings.")]
#[get("")]
pub async fn anonymous_list_handler(
    path: web::Path<()>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::rating::fetch_all_visible(pg_pool.get_ref())
        .await
        .map(|ratings| JsonResponse::build().set_list(ratings).ok("OK"))
        .map_err(|_err| JsonResponse::<models::Rating>::build().internal_server_error(""))
}

#[tracing::instrument(name = "Admin get rating.")]
#[get("/{id}")]
pub async fn admin_get_handler(
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let rate_id = path.0;
    let rating = db::rating::fetch(pg_pool.get_ref(), rate_id)
        .await
        .map_err(|_err| JsonResponse::<models::Rating>::build().internal_server_error(""))
        .and_then(|rating| {
            match rating {
                Some(rating) => { Ok(rating) },
                _ => Err(JsonResponse::<models::Rating>::build().not_found("not found"))
            }
        })?;

    Ok(JsonResponse::build().set_item(rating).ok("OK"))
}

#[tracing::instrument(name = "Admin get the list of ratings.")]
#[get("")]
pub async fn admin_list_handler(
    path: web::Path<()>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::rating::fetch_all(pg_pool.get_ref())
        .await
        .map(|ratings| JsonResponse::build().set_list(ratings).ok("OK"))
        .map_err(|_err| JsonResponse::<models::Rating>::build().internal_server_error(""))
}
