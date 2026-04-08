use crate::db;
use crate::helpers::JsonResponse;
use crate::models::User;
use actix_web::{get, web, Responder, Result};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct ListTemplatesQuery {
    pub source_app_type: Option<String>,
    pub target_app_type: Option<String>,
    #[serde(default)]
    pub public_only: bool,
}

#[tracing::instrument(name = "List pipe templates", skip_all)]
#[get("/templates")]
pub async fn list_templates_handler(
    _user: web::ReqData<Arc<User>>,
    query: web::Query<ListTemplatesQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let templates = db::pipe::list_templates(
        pg_pool.get_ref(),
        query.source_app_type.as_deref(),
        query.target_app_type.as_deref(),
        query.public_only,
    )
    .await
    .map_err(|err| {
        tracing::error!("Failed to list pipe templates: {}", err);
        JsonResponse::internal_server_error(err)
    })?;

    Ok(JsonResponse::build()
        .set_list(templates)
        .ok("Pipe templates fetched successfully"))
}

#[tracing::instrument(name = "List pipe instances for deployment", skip_all)]
#[get("/instances/{deployment_hash}")]
pub async fn list_instances_handler(
    _user: web::ReqData<Arc<User>>,
    path: web::Path<String>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let deployment_hash = path.into_inner();

    let instances = db::pipe::list_instances(pg_pool.get_ref(), &deployment_hash)
        .await
        .map_err(|err| {
            tracing::error!("Failed to list pipe instances: {}", err);
            JsonResponse::internal_server_error(err)
        })?;

    Ok(JsonResponse::build()
        .set_list(instances)
        .ok("Pipe instances fetched successfully"))
}
