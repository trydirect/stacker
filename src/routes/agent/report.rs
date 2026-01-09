use crate::{db, forms::status_panel, helpers, models};
use actix_web::{post, web, HttpRequest, Responder, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct CommandReportRequest {
    pub command_id: String,
    pub deployment_hash: String,
    pub status: String, // domain-level status (e.g., ok|unhealthy|failed)
    #[serde(default)]
    pub command_status: Option<String>, // explicitly force completed/failed
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
    #[serde(default)]
    pub errors: Option<Vec<serde_json::Value>>, // preferred multi-error payload
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
        return Err(helpers::JsonResponse::forbidden(
            "Not authorized for this deployment",
        ));
    }

    // Update agent heartbeat
    let _ = db::agent::update_heartbeat(pg_pool.get_ref(), agent.id, "online").await;

    // Parse status to CommandStatus enum
    let has_errors = payload
        .errors
        .as_ref()
        .map(|errs| !errs.is_empty())
        .unwrap_or(false);

    let status = match payload.command_status.as_deref() {
        Some(value) => match value.to_lowercase().as_str() {
            "completed" => models::CommandStatus::Completed,
            "failed" => models::CommandStatus::Failed,
            _ => {
                return Err(helpers::JsonResponse::bad_request(
                    "Invalid command_status. Must be 'completed' or 'failed'",
                ));
            }
        },
        None => {
            if payload.status.eq_ignore_ascii_case("failed") || has_errors {
                models::CommandStatus::Failed
            } else {
                models::CommandStatus::Completed
            }
        }
    };

    let command = db::command::fetch_by_id(pg_pool.get_ref(), &payload.command_id)
        .await
        .map_err(|err| {
            tracing::error!("Failed to fetch command {}: {}", payload.command_id, err);
            helpers::JsonResponse::internal_server_error(err)
        })?;

    let command = match command {
        Some(cmd) => cmd,
        None => {
            tracing::warn!("Command not found for report: {}", payload.command_id);
            return Err(helpers::JsonResponse::not_found("Command not found"));
        }
    };

    if command.deployment_hash != payload.deployment_hash {
        tracing::warn!(
            "Deployment hash mismatch for command {}: expected {}, got {}",
            payload.command_id,
            command.deployment_hash,
            payload.deployment_hash
        );
        return Err(helpers::JsonResponse::not_found(
            "Command not found for this deployment",
        ));
    }

    let error_payload = if let Some(errors) = payload.errors.as_ref() {
        if errors.is_empty() {
            None
        } else {
            Some(json!({ "errors": errors }))
        }
    } else {
        payload.error.clone()
    };

    let mut result_payload = status_panel::validate_command_result(
        &command.r#type,
        &payload.deployment_hash,
        &payload.result,
    )
    .map_err(|err| {
        tracing::warn!(
            command_type = %command.r#type,
            command_id = %payload.command_id,
            "Invalid command result payload: {}",
            err
        );
        helpers::JsonResponse::<()>::build().bad_request(err)
    })?;

    if result_payload.is_none() && !payload.status.is_empty() {
        result_payload = Some(json!({ "status": payload.status.clone() }));
    }

    // Update command in database with result
    match db::command::update_result(
        pg_pool.get_ref(),
        &payload.command_id,
        &status,
        result_payload.clone(),
        error_payload.clone(),
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
                "has_result": result_payload.is_some(),
                "has_error": error_payload.is_some(),
                "reported_status": payload.status,
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
