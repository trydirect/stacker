use crate::{db, helpers, models};
use actix_web::{post, web, HttpRequest, Responder, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct CommandReportRequest {
    pub command_id: String,
    pub deployment_hash: String,
    pub status: String, // "completed" or "failed"
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Default)]
pub struct CommandReportResponse {
    pub accepted: bool,
    pub message: String,
}

#[tracing::instrument(name = "Agent report command result", skip(pg_pool, _req))]
#[post("/commands/report")]
pub async fn report_handler(
    agent: web::ReqData<Arc<models::Agent>>,
    payload: web::Json<CommandReportRequest>,
    pg_pool: web::Data<PgPool>,
    _req: HttpRequest,
) -> Result<impl Responder> {
    // Verify agent is authorized for this deployment_hash
    if agent.deployment_hash != payload.deployment_hash {
        return Err(helpers::JsonResponse::forbidden("Not authorized for this deployment"));
    }

    // Validate status
    if payload.status != "completed" && payload.status != "failed" {
        return Err(helpers::JsonResponse::bad_request(
            "Invalid status. Must be 'completed' or 'failed'"
        ));
    }

    // Update agent heartbeat
    let _ = db::agent::update_heartbeat(pg_pool.get_ref(), agent.id, "online").await;

    // Parse status to CommandStatus enum
    let status = match payload.status.to_lowercase().as_str() {
        "completed" => models::CommandStatus::Completed,
        "failed" => models::CommandStatus::Failed,
        _ => {
            return Err(helpers::JsonResponse::bad_request(
                "Invalid status. Must be 'completed' or 'failed'"
            ));
        }
    };

    // Update command in database with result
    match db::command::update_result(
        pg_pool.get_ref(),
        &payload.command_id,
        &status,
        payload.result.clone(),
        payload.error.clone(),
    )
    .await
    {
        Ok(_) => {
            tracing::info!(
                "Command {} updated to status '{}' by agent {}",
                payload.command_id,
                status,
                agent.id
            );

            // Remove from queue if still there (shouldn't be, but cleanup)
            let _ = db::command::remove_from_queue(pg_pool.get_ref(), &payload.command_id).await;

            // Log audit event
            let audit_log = models::AuditLog::new(
                Some(agent.id),
                Some(payload.deployment_hash.clone()),
                "agent.command_reported".to_string(),
                Some(status.to_string()),
            )
            .with_details(serde_json::json!({
                "command_id": payload.command_id,
                "status": status.to_string(),
                "has_result": payload.result.is_some(),
                "has_error": payload.error.is_some(),
            }));

            let _ = db::agent::log_audit(pg_pool.get_ref(), audit_log).await;

            let response = CommandReportResponse {
                accepted: true,
                message: format!("Command result accepted, status: {}", status),
            };

            Ok(helpers::JsonResponse::build()
                .set_item(Some(response))
                .ok("Result accepted"))
        }
        Err(err) => {
            tracing::error!(
                "Failed to update command {} result: {}",
                payload.command_id,
                err
            );

            // Log failure in audit log
            let audit_log = models::AuditLog::new(
                Some(agent.id),
                Some(payload.deployment_hash.clone()),
                "agent.command_report_failed".to_string(),
                Some("error".to_string()),
            )
            .with_details(serde_json::json!({
                "command_id": payload.command_id,
                "error": err,
            }));

            let _ = db::agent::log_audit(pg_pool.get_ref(), audit_log).await;

            Err(helpers::JsonResponse::internal_server_error(err))
        }
    }
}
