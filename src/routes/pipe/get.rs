use crate::db;
use crate::helpers::JsonResponse;
use crate::models::User;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;

#[tracing::instrument(name = "Get pipe template by ID", skip(pg_pool, _user))]
#[get("/templates/{template_id}")]
pub async fn get_template_handler(
    _user: web::ReqData<Arc<User>>,
    path: web::Path<uuid::Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let template_id = path.into_inner();

    let template = db::pipe::get_template(pg_pool.get_ref(), &template_id)
        .await
        .map_err(|err| {
            tracing::error!("Failed to fetch pipe template: {}", err);
            JsonResponse::internal_server_error(err)
        })?;

    match template {
        Some(t) => Ok(JsonResponse::build()
            .set_item(Some(t))
            .ok("Pipe template fetched successfully")),
        None => Err(JsonResponse::not_found("Pipe template not found")),
    }
}

#[tracing::instrument(name = "Get pipe instance by ID", skip(pg_pool, _user))]
#[get("/instances/detail/{instance_id}")]
pub async fn get_instance_handler(
    _user: web::ReqData<Arc<User>>,
    path: web::Path<uuid::Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let instance_id = path.into_inner();

    let instance = db::pipe::get_instance(pg_pool.get_ref(), &instance_id)
        .await
        .map_err(|err| {
            tracing::error!("Failed to fetch pipe instance: {}", err);
            JsonResponse::internal_server_error(err)
        })?;

    match instance {
        Some(i) => Ok(JsonResponse::build()
            .set_item(Some(i))
            .ok("Pipe instance fetched successfully")),
        None => Err(JsonResponse::not_found("Pipe instance not found")),
    }
}
