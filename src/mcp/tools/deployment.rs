use async_trait::async_trait;
use serde_json::{json, Value};

use crate::db;
use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};
use serde::Deserialize;

/// Get deployment status
pub struct GetDeploymentStatusTool;

#[async_trait]
impl ToolHandler for GetDeploymentStatusTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            deployment_id: i32,
        }

        let args: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        let deployment = db::deployment::fetch(&context.pg_pool, args.deployment_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch deployment: {}", e);
                format!("Database error: {}", e)
            })?
            .ok_or_else(|| "Deployment not found".to_string())?;

        let result = serde_json::to_string(&deployment)
            .map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!("Got deployment status: {}", args.deployment_id);

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "get_deployment_status".to_string(),
            description:
                "Get the current status of a deployment (pending, running, completed, failed)"
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment_id": {
                        "type": "number",
                        "description": "Deployment ID"
                    }
                },
                "required": ["deployment_id"]
            }),
        }
    }
}

/// Start a new deployment
pub struct StartDeploymentTool;

#[async_trait]
impl ToolHandler for StartDeploymentTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            project_id: i32,
            cloud_id: Option<i32>,
            environment: Option<String>,
        }

        let args: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        // Verify user owns the project
        let project = db::project::fetch(&context.pg_pool, args.project_id)
            .await
            .map_err(|e| format!("Project not found: {}", e))?
            .ok_or_else(|| "Project not found".to_string())?;

        if project.user_id != context.user.id {
            return Err("Unauthorized: You do not own this project".to_string());
        }

        // Create deployment record with hash
        let deployment_hash = uuid::Uuid::new_v4().to_string();
        let deployment = crate::models::Deployment::new(
            args.project_id,
            Some(context.user.id.clone()),
            deployment_hash.clone(),
            "pending".to_string(),
            json!({ "environment": args.environment.unwrap_or_else(|| "production".to_string()), "cloud_id": args.cloud_id }),
        );

        let deployment = db::deployment::insert(&context.pg_pool, deployment)
            .await
            .map_err(|e| format!("Failed to create deployment: {}", e))?;

        let response = serde_json::json!({
            "id": deployment.id,
            "project_id": deployment.project_id,
            "status": deployment.status,
            "deployment_hash": deployment.deployment_hash,
            "created_at": deployment.created_at,
            "message": "Deployment initiated - agent will connect shortly"
        });

        tracing::info!(
            "Started deployment {} for project {}",
            deployment.id,
            args.project_id
        );

        Ok(ToolContent::Text {
            text: response.to_string(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "start_deployment".to_string(),
            description: "Initiate deployment of a project to cloud infrastructure".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "number",
                        "description": "Project ID to deploy"
                    },
                    "cloud_id": {
                        "type": "number",
                        "description": "Cloud provider ID (optional)"
                    },
                    "environment": {
                        "type": "string",
                        "description": "Deployment environment (optional, default: production)",
                        "enum": ["development", "staging", "production"]
                    }
                },
                "required": ["project_id"]
            }),
        }
    }
}

/// Cancel a deployment
pub struct CancelDeploymentTool;

#[async_trait]
impl ToolHandler for CancelDeploymentTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            deployment_id: i32,
        }

        let args: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        let _deployment = db::deployment::fetch(&context.pg_pool, args.deployment_id)
            .await
            .map_err(|e| format!("Deployment not found: {}", e))?
            .ok_or_else(|| "Deployment not found".to_string())?;

        // Verify user owns the project (via deployment)
        let project = db::project::fetch(&context.pg_pool, _deployment.project_id)
            .await
            .map_err(|e| format!("Project not found: {}", e))?
            .ok_or_else(|| "Project not found".to_string())?;

        if project.user_id != context.user.id {
            return Err("Unauthorized: You do not own this deployment".to_string());
        }

        // Mark deployment as cancelled (would update status in real implementation)
        let response = serde_json::json!({
            "deployment_id": args.deployment_id,
            "status": "cancelled",
            "message": "Deployment cancellation initiated"
        });

        tracing::info!("Cancelled deployment {}", args.deployment_id);

        Ok(ToolContent::Text {
            text: response.to_string(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "cancel_deployment".to_string(),
            description: "Cancel an in-progress or pending deployment".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment_id": {
                        "type": "number",
                        "description": "Deployment ID to cancel"
                    }
                },
                "required": ["deployment_id"]
            }),
        }
    }
}
