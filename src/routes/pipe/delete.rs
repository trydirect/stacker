use crate::db;
use crate::helpers::JsonResponse;
use crate::models::User;
use actix_web::{delete, web, Responder, Result};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Serialize)]
struct DeleteResponse {
    deleted: bool,
}

#[tracing::instrument(name = "Delete pipe template", skip(pg_pool, _user))]
#[delete("/templates/{template_id}")]
pub async fn delete_template_handler(
    _user: web::ReqData<Arc<User>>,
    path: web::Path<uuid::Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let template_id = path.into_inner();

    let deleted = db::pipe::delete_template(pg_pool.get_ref(), &template_id)
        .await
        .map_err(|err| {
            tracing::error!("Failed to delete pipe template: {}", err);
            JsonResponse::<()>::build().internal_server_error(err)
        })?;

    if deleted {
        Ok(JsonResponse::build()
            .set_item(Some(DeleteResponse { deleted: true }))
            .ok("Pipe template deleted successfully"))
    } else {
        Err(JsonResponse::not_found("Pipe template not found"))
    }
}

#[tracing::instrument(name = "Delete pipe instance", skip(pg_pool, _user))]
#[delete("/instances/{instance_id}")]
pub async fn delete_instance_handler(
    _user: web::ReqData<Arc<User>>,
    path: web::Path<uuid::Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let instance_id = path.into_inner();

    let deleted = db::pipe::delete_instance(pg_pool.get_ref(), &instance_id)
        .await
        .map_err(|err| {
            tracing::error!("Failed to delete pipe instance: {}", err);
            JsonResponse::<()>::build().internal_server_error(err)
        })?;

    if deleted {
        Ok(JsonResponse::build()
            .set_item(Some(DeleteResponse { deleted: true }))
            .ok("Pipe instance deleted successfully"))
    } else {
        Err(JsonResponse::not_found("Pipe instance not found"))
    }
}
