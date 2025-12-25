use crate::db;
use crate::helpers::JsonResponse;
use crate::models::User;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;

#[tracing::instrument(name = "Get command by ID", skip(pg_pool, user))]
#[get("/{deployment_hash}/{command_id}")]
pub async fn get_handler(
    user: web::ReqData<Arc<User>>,
    path: web::Path<(String, String)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (deployment_hash, command_id) = path.into_inner();

    // Fetch command
    let command = db::command::fetch_by_id(pg_pool.get_ref(), &command_id)
        .await
        .map_err(|err| {
            tracing::error!("Failed to fetch command: {}", err);
            JsonResponse::internal_server_error(err)
        })?;

    match command {
        Some(cmd) => {
            // Verify deployment_hash matches (authorization check)
            if cmd.deployment_hash != deployment_hash {
                tracing::warn!(
                    "Deployment hash mismatch: expected {}, got {}",
                    deployment_hash,
                    cmd.deployment_hash
                );
                return Err(JsonResponse::not_found(
                    "Command not found for this deployment",
                ));
            }

            tracing::info!(
                "Fetched command {} for deployment {} by user {}",
                command_id,
                deployment_hash,
                user.id
            );

            Ok(JsonResponse::build()
                .set_item(Some(cmd))
                .ok("Command fetched successfully"))
        }
        None => {
            tracing::warn!("Command not found: {}", command_id);
            Err(JsonResponse::not_found("Command not found"))
        }
    }
}
