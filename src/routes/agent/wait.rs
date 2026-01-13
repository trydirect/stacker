use crate::{db, helpers, models};
use actix_web::{get, web, HttpRequest, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

#[tracing::instrument(name = "Agent poll for commands", skip(pg_pool, _req))]
#[get("/commands/wait/{deployment_hash}")]
pub async fn wait_handler(
    agent: web::ReqData<Arc<models::Agent>>,
    path: web::Path<String>,
    pg_pool: web::Data<PgPool>,
    _req: HttpRequest,
) -> Result<impl Responder> {
    let deployment_hash = path.into_inner();

    // Verify agent is authorized for this deployment_hash
    if agent.deployment_hash != deployment_hash {
        return Err(helpers::JsonResponse::forbidden(
            "Not authorized for this deployment",
        ));
    }

    // Update agent heartbeat - acquire and release connection quickly
    let _ = db::agent::update_heartbeat(pg_pool.get_ref(), agent.id, "online").await;

    // Log poll event - acquire and release connection quickly
    let audit_log = models::AuditLog::new(
        Some(agent.id),
        Some(deployment_hash.clone()),
        "agent.command_polled".to_string(),
        Some("success".to_string()),
    );
    let _ = db::agent::log_audit(pg_pool.get_ref(), audit_log).await;

    // Long-polling: Check for pending commands with retries
    // IMPORTANT: Each check acquires and releases DB connection to avoid pool exhaustion
    let timeout_seconds = 30;
    let check_interval = Duration::from_secs(2);
    let max_checks = timeout_seconds / check_interval.as_secs();

    for i in 0..max_checks {
        // Acquire connection only for query, then release immediately
        match db::command::fetch_next_for_deployment(pg_pool.get_ref(), &deployment_hash).await {
            Ok(Some(command)) => {
                tracing::info!(
                    "Found command {} for agent {} (deployment {})",
                    command.command_id,
                    agent.id,
                    deployment_hash
                );

                // Update command status to 'sent' - separate connection
                let updated_command = db::command::update_status(
                    pg_pool.get_ref(),
                    &command.command_id,
                    &models::CommandStatus::Sent,
                )
                .await
                .map_err(|err| {
                    tracing::error!("Failed to update command status: {}", err);
                    helpers::JsonResponse::internal_server_error(err)
                })?;

                // Remove from queue - separate connection
                let _ =
                    db::command::remove_from_queue(pg_pool.get_ref(), &command.command_id).await;

                return Ok(helpers::JsonResponse::<Option<models::Command>>::build()
                    .set_item(Some(updated_command))
                    .ok("Command available"));
            }
            Ok(None) => {
                // No command yet, sleep WITHOUT holding DB connection
                if i < max_checks - 1 {
                    tokio::time::sleep(check_interval).await;
                }
            }
            Err(err) => {
                tracing::error!("Failed to fetch command from queue: {}", err);
                return Err(helpers::JsonResponse::internal_server_error(err));
            }
        }
    }

    // No commands available after timeout
    tracing::debug!(
        "No commands available for agent {} after {} seconds",
        agent.id,
        timeout_seconds
    );
    Ok(helpers::JsonResponse::<Option<models::Command>>::build()
        .set_item(None)
        .ok("No command available"))
}
