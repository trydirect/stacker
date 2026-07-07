use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{delete, get, put, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, serde::Deserialize)]
pub struct TemplateRatingRequest {
    pub rating: i32,
    pub comment: Option<String>,
}

fn parse_template_id(raw: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(raw).map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))
}

fn validate_template_rating_request(req: &TemplateRatingRequest) -> Result<()> {
    if !(1..=5).contains(&req.rating) {
        return Err(JsonResponse::<serde_json::Value>::build()
            .bad_request("rating must be between 1 and 5"));
    }

    if req
        .comment
        .as_ref()
        .map(|comment| comment.chars().count() > 1000)
        .unwrap_or(false)
    {
        return Err(JsonResponse::<serde_json::Value>::build()
            .bad_request("comment must be at most 1000 characters"));
    }

    Ok(())
}

#[tracing::instrument(name = "Get public template rating summary", skip_all)]
#[get("/{id}/rating/summary")]
pub async fn summary_handler(
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let template_id = parse_template_id(&path.into_inner().0)?;

    let summary = db::rating::template_summary(pg_pool.get_ref(), &template_id)
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?
        .ok_or_else(|| {
            JsonResponse::<serde_json::Value>::build().not_found("Template not found")
        })?;

    Ok(JsonResponse::build().set_item(summary).ok("OK"))
}

#[tracing::instrument(name = "Get my template rating", skip_all)]
#[get("/{id}/rating/me")]
pub async fn my_rating_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let template_id = parse_template_id(&path.into_inner().0)?;

    let rating =
        db::rating::fetch_template_rating_for_user(pg_pool.get_ref(), &template_id, &user.id)
            .await
            .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?
            .ok_or_else(|| {
                JsonResponse::<serde_json::Value>::build().not_found("Rating not found")
            })?;

    Ok(JsonResponse::build().set_item(rating).ok("OK"))
}

#[tracing::instrument(name = "Upsert my template rating", skip_all)]
#[put("/{id}/rating")]
pub async fn upsert_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
    body: web::Json<TemplateRatingRequest>,
) -> Result<impl Responder> {
    let template_id = parse_template_id(&path.into_inner().0)?;
    let req = body.into_inner();
    validate_template_rating_request(&req)?;

    let rating = db::rating::upsert_template_rating_for_user(
        pg_pool.get_ref(),
        &template_id,
        &user.id,
        req.rating,
        req.comment,
    )
    .await
    .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?
    .ok_or_else(|| JsonResponse::<serde_json::Value>::build().not_found("Template not found"))?;

    Ok(JsonResponse::build().set_item(rating).ok("OK"))
}

#[tracing::instrument(name = "Delete my template rating", skip_all)]
#[delete("/{id}/rating")]
pub async fn delete_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let template_id = parse_template_id(&path.into_inner().0)?;

    db::rating::hide_template_rating_for_user(pg_pool.get_ref(), &template_id, &user.id)
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?
        .ok_or_else(|| JsonResponse::<serde_json::Value>::build().not_found("Rating not found"))?;

    Ok(JsonResponse::<serde_json::Value>::build().ok("Rating deleted"))
}
