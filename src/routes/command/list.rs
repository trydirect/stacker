use crate::db;
use crate::helpers::JsonResponse;
use crate::models::{Command, User};
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;

#[tracing::instrument(name = "List commands for deployment", skip(pg_pool, user))]
#[get("/{deployment_hash}")]
pub async fn list_handler(
    user: web::ReqData<Arc<User>>,
    path: web::Path<String>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let deployment_hash = path.into_inner();

    // Fetch all commands for this deployment
    let commands = db::command::fetch_by_deployment(pg_pool.get_ref(), &deployment_hash)
        .await
        .map_err(|err| {
            tracing::error!("Failed to fetch commands: {}", err);
            JsonResponse::internal_server_error(err)
        })?;

    tracing::info!(
        "Fetched {} commands for deployment {} by user {}",
        commands.len(),
        deployment_hash,
        user.id
    );

    Ok(JsonResponse::build()
        .set_list(commands)
        .ok("Commands fetched successfully"))
}
