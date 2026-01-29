//! MCP Tools for Logs & Monitoring via Status Agent.
//!
//! These tools provide AI access to:
//! - Container logs (paginated, redacted)
//! - Container health metrics (CPU, RAM, network)
//! - Deployment-wide container status
//!
//! Commands are dispatched to Status Agent via Stacker's agent communication layer.
//!
//! Deployment resolution is handled via `DeploymentIdentifier` which supports:
//! - Stack Builder deployments (deployment_hash directly)
//! - User Service installations (deployment_id → lookup hash via connector)

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::connectors::user_service::UserServiceDeploymentResolver;
use crate::db;
use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};
use crate::models::{Command, CommandPriority};
use crate::services::{DeploymentIdentifier, DeploymentResolver};
use serde::Deserialize;

const DEFAULT_LOG_LIMIT: usize = 100;
const MAX_LOG_LIMIT: usize = 500;

/// Helper to create a resolver from context.
/// Uses UserServiceDeploymentResolver from connectors to support legacy installations.
fn create_resolver(context: &ToolContext) -> UserServiceDeploymentResolver {
    UserServiceDeploymentResolver::from_context(
        &context.settings.user_service_url,
        context.user.access_token.as_deref(),
    )
}

/// Get container logs from a deployment
pub struct GetContainerLogsTool;

#[async_trait]
impl ToolHandler for GetContainerLogsTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            deployment_id: Option<i64>,
            #[serde(default)]
            deployment_hash: Option<String>,
            #[serde(default)]
            app_code: Option<String>,
            #[serde(default)]
            limit: Option<usize>,
            #[serde(default)]
            cursor: Option<String>,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        // Create identifier from args (prefers hash if both provided)
        let identifier =
            DeploymentIdentifier::try_from_options(params.deployment_hash, params.deployment_id)?;

        // Resolve to deployment_hash
        let resolver = create_resolver(context);
        let deployment_hash = resolver.resolve(&identifier).await?;

        let limit = params.limit.unwrap_or(DEFAULT_LOG_LIMIT).min(MAX_LOG_LIMIT);

        // Create command for agent
        let command_id = uuid::Uuid::new_v4().to_string();
        let command = Command::new(
            command_id.clone(),
            deployment_hash.clone(),
            "logs".to_string(),
            context.user.id.clone(),
        )
        .with_parameters(json!({
            "name": "stacker.logs",
            "params": {
                "deployment_hash": deployment_hash,
                "app_code": params.app_code.clone().unwrap_or_default(),
                "limit": limit,
                "cursor": params.cursor,
                "redact": true  // Always redact for AI safety
            }
        }));

        // Insert command and add to queue
        let command = db::command::insert(&context.pg_pool, &command)
            .await
            .map_err(|e| format!("Failed to create command: {}", e))?;

        db::command::add_to_queue(
            &context.pg_pool,
            &command.command_id,
            &deployment_hash,
            &CommandPriority::Normal,
        )
        .await
        .map_err(|e| format!("Failed to queue command: {}", e))?;

        // For now, return acknowledgment (agent will process async)
        // In production, we'd wait for result with timeout
        let result = json!({
            "status": "queued",
            "command_id": command.command_id,
            "deployment_hash": deployment_hash,
            "app_code": params.app_code,
            "limit": limit,
            "message": "Log request queued. Agent will process shortly."
        });

        tracing::info!(
            user_id = %context.user.id,
            deployment_id = ?params.deployment_id,
            deployment_hash = %deployment_hash,
            "Queued logs command via MCP"
        );

        Ok(ToolContent::Text {
            text: result.to_string(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "get_container_logs".to_string(),
            description: "Fetch container logs from a deployment. Logs are automatically redacted to remove sensitive information like passwords and API keys.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment_id": {
                        "type": "number",
                        "description": "The deployment/installation ID (for legacy User Service deployments)"
                    },
                    "deployment_hash": {
                        "type": "string",
                        "description": "The deployment hash (for Stack Builder deployments). Use this if available in context."
                    },
                    "app_code": {
                        "type": "string",
                        "description": "Specific app/container to get logs from (e.g., 'nginx', 'postgres'). If omitted, returns logs from all containers."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of log lines to return (default: 100, max: 500)"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Pagination cursor for fetching more logs"
                    }
                },
                "required": []
            }),
        }
    }
}

/// Get container health metrics from a deployment
pub struct GetContainerHealthTool;

#[async_trait]
impl ToolHandler for GetContainerHealthTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            deployment_id: Option<i64>,
            #[serde(default)]
            deployment_hash: Option<String>,
            #[serde(default)]
            app_code: Option<String>,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        // Create identifier and resolve to hash
        let identifier =
            DeploymentIdentifier::try_from_options(params.deployment_hash, params.deployment_id)?;
        let resolver = create_resolver(context);
        let deployment_hash = resolver.resolve(&identifier).await?;

        // Create health command for agent
        let command_id = uuid::Uuid::new_v4().to_string();
        let command = Command::new(
            command_id.clone(),
            deployment_hash.clone(),
            "health".to_string(),
            context.user.id.clone(),
        )
        .with_parameters(json!({
            "name": "stacker.health",
            "params": {
                "deployment_hash": deployment_hash,
                "app_code": params.app_code.clone().unwrap_or_default(),
                "include_metrics": true
            }
        }));

        let command = db::command::insert(&context.pg_pool, &command)
            .await
            .map_err(|e| format!("Failed to create command: {}", e))?;

        db::command::add_to_queue(
            &context.pg_pool,
            &command.command_id,
            &deployment_hash,
            &CommandPriority::Normal,
        )
        .await
        .map_err(|e| format!("Failed to queue command: {}", e))?;

        let result = json!({
            "status": "queued",
            "command_id": command.command_id,
            "deployment_hash": deployment_hash,
            "app_code": params.app_code,
            "message": "Health check queued. Agent will process shortly."
        });

        tracing::info!(
            user_id = %context.user.id,
            deployment_id = ?params.deployment_id,
            deployment_hash = %deployment_hash,
            "Queued health command via MCP"
        );

        Ok(ToolContent::Text {
            text: result.to_string(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "get_container_health".to_string(),
            description: "Get health metrics for containers in a deployment including CPU usage, memory usage, network I/O, and uptime.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment_id": {
                        "type": "number",
                        "description": "The deployment/installation ID (for legacy User Service deployments)"
                    },
                    "deployment_hash": {
                        "type": "string",
                        "description": "The deployment hash (for Stack Builder deployments). Use this if available in context."
                    },
                    "app_code": {
                        "type": "string",
                        "description": "Specific app/container to check (e.g., 'nginx', 'postgres'). If omitted, returns health for all containers."
                    }
                },
                "required": []
            }),
        }
    }
}

/// Restart a container in a deployment
pub struct RestartContainerTool;

#[async_trait]
impl ToolHandler for RestartContainerTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            deployment_id: Option<i64>,
            #[serde(default)]
            deployment_hash: Option<String>,
            app_code: String,
            #[serde(default)]
            force: bool,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        if params.app_code.trim().is_empty() {
            return Err("app_code is required to restart a specific container".to_string());
        }

        // Create identifier and resolve to hash
        let identifier =
            DeploymentIdentifier::try_from_options(params.deployment_hash, params.deployment_id)?;
        let resolver = create_resolver(context);
        let deployment_hash = resolver.resolve(&identifier).await?;

        // Create restart command for agent
        let command_id = uuid::Uuid::new_v4().to_string();
        let command = Command::new(
            command_id.clone(),
            deployment_hash.clone(),
            "restart".to_string(),
            context.user.id.clone(),
        )
        .with_priority(CommandPriority::High) // Restart is high priority
        .with_parameters(json!({
            "name": "stacker.restart",
            "params": {
                "deployment_hash": deployment_hash,
                "app_code": params.app_code.clone(),
                "force": params.force
            }
        }));

        let command = db::command::insert(&context.pg_pool, &command)
            .await
            .map_err(|e| format!("Failed to create command: {}", e))?;

        db::command::add_to_queue(
            &context.pg_pool,
            &command.command_id,
            &deployment_hash,
            &CommandPriority::High,
        )
        .await
        .map_err(|e| format!("Failed to queue command: {}", e))?;

        let result = json!({
            "status": "queued",
            "command_id": command.command_id,
            "deployment_hash": deployment_hash,
            "app_code": params.app_code,
            "message": format!("Restart command for '{}' queued. Container will restart shortly.", params.app_code)
        });

        tracing::warn!(
            user_id = %context.user.id,
            deployment_id = ?params.deployment_id,
            deployment_hash = %deployment_hash,
            app_code = %params.app_code,
            "Queued RESTART command via MCP"
        );

        Ok(ToolContent::Text {
            text: result.to_string(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "restart_container".to_string(),
            description: "Restart a specific container in a deployment. This is a potentially disruptive action - use when a container is unhealthy or needs to pick up configuration changes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment_id": {
                        "type": "number",
                        "description": "The deployment/installation ID (for legacy User Service deployments)"
                    },
                    "deployment_hash": {
                        "type": "string",
                        "description": "The deployment hash (for Stack Builder deployments). Use this if available in context."
                    },
                    "app_code": {
                        "type": "string",
                        "description": "The app/container code to restart (e.g., 'nginx', 'postgres')"
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Force restart even if container appears healthy (default: false)"
                    }
                },
                "required": ["app_code"]
            }),
        }
    }
}

/// Diagnose deployment issues
pub struct DiagnoseDeploymentTool;

#[async_trait]
impl ToolHandler for DiagnoseDeploymentTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            deployment_id: Option<i64>,
            #[serde(default)]
            deployment_hash: Option<String>,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        // Create identifier and resolve with full info
        let identifier =
            DeploymentIdentifier::try_from_options(params.deployment_hash, params.deployment_id)?;
        let resolver = create_resolver(context);
        let info = resolver.resolve_with_info(&identifier).await?;

        let deployment_hash = info.deployment_hash;
        let status = info.status;
        let domain = info.domain;
        let server_ip = info.server_ip;
        let apps = info.apps;

        // Build diagnostic summary
        let mut issues: Vec<String> = Vec::new();
        let mut recommendations: Vec<String> = Vec::new();

        // Check deployment status
        match status.as_str() {
            "failed" => {
                issues.push("Deployment is in FAILED state".to_string());
                recommendations.push("Check deployment logs for error details".to_string());
                recommendations.push("Verify cloud credentials are valid".to_string());
            }
            "pending" => {
                issues.push("Deployment is still PENDING".to_string());
                recommendations.push(
                    "Wait for deployment to complete or check for stuck processes".to_string(),
                );
            }
            "running" | "completed" => {
                // Deployment looks healthy from our perspective
            }
            s => {
                issues.push(format!("Deployment has unusual status: {}", s));
            }
        }

        // Check if agent is connected (check last heartbeat)
        if let Ok(Some(agent)) =
            db::agent::fetch_by_deployment_hash(&context.pg_pool, &deployment_hash).await
        {
            if let Some(last_seen) = agent.last_heartbeat {
                let now = chrono::Utc::now();
                let diff = now.signed_duration_since(last_seen);
                if diff.num_minutes() > 5 {
                    issues.push(format!(
                        "Agent last seen {} minutes ago - may be offline",
                        diff.num_minutes()
                    ));
                    recommendations.push(
                        "Check if server is running and has network connectivity".to_string(),
                    );
                }
            }
        } else {
            issues.push("No agent registered for this deployment".to_string());
            recommendations
                .push("Ensure the Status Agent is installed and running on the server".to_string());
        }

        let result = json!({
            "deployment_id": params.deployment_id,
            "deployment_hash": deployment_hash,
            "status": status,
            "domain": domain,
            "server_ip": server_ip,
            "apps": apps,
            "issues_found": issues.len(),
            "issues": issues,
            "recommendations": recommendations,
            "next_steps": if issues.is_empty() {
                vec!["Deployment appears healthy. Use get_container_health for detailed metrics.".to_string()]
            } else {
                vec!["Address the issues above, then re-run diagnosis.".to_string()]
            }
        });

        tracing::info!(
            user_id = %context.user.id,
            deployment_id = ?params.deployment_id,
            deployment_hash = %deployment_hash,
            issues = issues.len(),
            "Ran deployment diagnosis via MCP"
        );

        Ok(ToolContent::Text {
            text: serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "diagnose_deployment".to_string(),
            description: "Run diagnostic checks on a deployment to identify potential issues. Returns a list of detected problems and recommended actions.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment_id": {
                        "type": "number",
                        "description": "The deployment/installation ID (for legacy User Service deployments)"
                    },
                    "deployment_hash": {
                        "type": "string",
                        "description": "The deployment hash (for Stack Builder deployments). Use this if available in context."
                    }
                },
                "required": []
            }),
        }
    }
}

/// Stop a container in a deployment
pub struct StopContainerTool;

#[async_trait]
impl ToolHandler for StopContainerTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            deployment_id: Option<i64>,
            #[serde(default)]
            deployment_hash: Option<String>,
            app_code: String,
            #[serde(default)]
            timeout: Option<u32>,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        if params.app_code.trim().is_empty() {
            return Err("app_code is required to stop a specific container".to_string());
        }

        // Create identifier and resolve to hash
        let identifier =
            DeploymentIdentifier::try_from_options(params.deployment_hash, params.deployment_id)?;
        let resolver = create_resolver(context);
        let deployment_hash = resolver.resolve(&identifier).await?;

        // Create stop command for agent
        let timeout = params.timeout.unwrap_or(30); // Default 30 second graceful shutdown
        let command_id = uuid::Uuid::new_v4().to_string();
        let command = Command::new(
            command_id.clone(),
            deployment_hash.clone(),
            "stop".to_string(),
            context.user.id.clone(),
        )
        .with_priority(CommandPriority::High)
        .with_parameters(json!({
            "name": "stacker.stop",
            "params": {
                "deployment_hash": deployment_hash,
                "app_code": params.app_code.clone(),
                "timeout": timeout
            }
        }));

        let command = db::command::insert(&context.pg_pool, &command)
            .await
            .map_err(|e| format!("Failed to create command: {}", e))?;

        db::command::add_to_queue(
            &context.pg_pool,
            &command.command_id,
            &deployment_hash,
            &CommandPriority::High,
        )
        .await
        .map_err(|e| format!("Failed to queue command: {}", e))?;

        let result = json!({
            "status": "queued",
            "command_id": command.command_id,
            "deployment_hash": deployment_hash,
            "app_code": params.app_code,
            "timeout": timeout,
            "message": format!("Stop command for '{}' queued. Container will stop within {} seconds.", params.app_code, timeout)
        });

        tracing::warn!(
            user_id = %context.user.id,
            deployment_id = ?params.deployment_id,
            deployment_hash = %deployment_hash,
            app_code = %params.app_code,
            "Queued STOP command via MCP"
        );

        Ok(ToolContent::Text {
            text: result.to_string(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "stop_container".to_string(),
            description: "Stop a specific container in a deployment. This will gracefully stop the container, allowing it to complete in-progress work. Use restart_container if you want to stop and start again.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment_id": {
                        "type": "number",
                        "description": "The deployment/installation ID (for legacy User Service deployments)"
                    },
                    "deployment_hash": {
                        "type": "string",
                        "description": "The deployment hash (for Stack Builder deployments). Use this if available in context."
                    },
                    "app_code": {
                        "type": "string",
                        "description": "The app/container code to stop (e.g., 'nginx', 'postgres')"
                    },
                    "timeout": {
                        "type": "number",
                        "description": "Graceful shutdown timeout in seconds (default: 30)"
                    }
                },
                "required": ["app_code"]
            }),
        }
    }
}

/// Start a stopped container in a deployment
pub struct StartContainerTool;

#[async_trait]
impl ToolHandler for StartContainerTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            deployment_id: Option<i64>,
            #[serde(default)]
            deployment_hash: Option<String>,
            app_code: String,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        if params.app_code.trim().is_empty() {
            return Err("app_code is required to start a specific container".to_string());
        }

        // Create identifier and resolve to hash
        let identifier =
            DeploymentIdentifier::try_from_options(params.deployment_hash, params.deployment_id)?;
        let resolver = create_resolver(context);
        let deployment_hash = resolver.resolve(&identifier).await?;

        // Create start command for agent
        let command_id = uuid::Uuid::new_v4().to_string();
        let command = Command::new(
            command_id.clone(),
            deployment_hash.clone(),
            "start".to_string(),
            context.user.id.clone(),
        )
        .with_priority(CommandPriority::High)
        .with_parameters(json!({
            "name": "stacker.start",
            "params": {
                "deployment_hash": deployment_hash,
                "app_code": params.app_code.clone()
            }
        }));

        let command = db::command::insert(&context.pg_pool, &command)
            .await
            .map_err(|e| format!("Failed to create command: {}", e))?;

        db::command::add_to_queue(
            &context.pg_pool,
            &command.command_id,
            &deployment_hash,
            &CommandPriority::High,
        )
        .await
        .map_err(|e| format!("Failed to queue command: {}", e))?;

        let result = json!({
            "status": "queued",
            "command_id": command.command_id,
            "deployment_hash": deployment_hash,
            "app_code": params.app_code,
            "message": format!("Start command for '{}' queued. Container will start shortly.", params.app_code)
        });

        tracing::info!(
            user_id = %context.user.id,
            deployment_id = ?params.deployment_id,
            deployment_hash = %deployment_hash,
            app_code = %params.app_code,
            "Queued START command via MCP"
        );

        Ok(ToolContent::Text {
            text: result.to_string(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "start_container".to_string(),
            description: "Start a stopped container in a deployment. Use this after stop_container to bring a container back online.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment_id": {
                        "type": "number",
                        "description": "The deployment/installation ID (for legacy User Service deployments)"
                    },
                    "deployment_hash": {
                        "type": "string",
                        "description": "The deployment hash (for Stack Builder deployments). Use this if available in context."
                    },
                    "app_code": {
                        "type": "string",
                        "description": "The app/container code to start (e.g., 'nginx', 'postgres')"
                    }
                },
                "required": ["app_code"]
            }),
        }
    }
}

/// Get a summary of errors from container logs
pub struct GetErrorSummaryTool;

#[async_trait]
impl ToolHandler for GetErrorSummaryTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            deployment_id: Option<i64>,
            #[serde(default)]
            deployment_hash: Option<String>,
            #[serde(default)]
            app_code: Option<String>,
            #[serde(default)]
            hours: Option<u32>,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        // Create identifier and resolve to hash
        let identifier =
            DeploymentIdentifier::try_from_options(params.deployment_hash, params.deployment_id)?;
        let resolver = create_resolver(context);
        let deployment_hash = resolver.resolve(&identifier).await?;

        let hours = params.hours.unwrap_or(24).min(168); // Max 7 days

        // Create error summary command for agent
        let command_id = uuid::Uuid::new_v4().to_string();
        let command = Command::new(
            command_id.clone(),
            deployment_hash.clone(),
            "error_summary".to_string(),
            context.user.id.clone(),
        )
        .with_parameters(json!({
            "name": "stacker.error_summary",
            "params": {
                "deployment_hash": deployment_hash,
                "app_code": params.app_code.clone().unwrap_or_default(),
                "hours": hours,
                "redact": true
            }
        }));

        let command = db::command::insert(&context.pg_pool, &command)
            .await
            .map_err(|e| format!("Failed to create command: {}", e))?;

        db::command::add_to_queue(
            &context.pg_pool,
            &command.command_id,
            &deployment_hash,
            &CommandPriority::Normal,
        )
        .await
        .map_err(|e| format!("Failed to queue command: {}", e))?;

        let result = json!({
            "status": "queued",
            "command_id": command.command_id,
            "deployment_hash": deployment_hash,
            "app_code": params.app_code,
            "hours": hours,
            "message": format!("Error summary request queued for the last {} hours. Agent will analyze logs shortly.", hours)
        });

        tracing::info!(
            user_id = %context.user.id,
            deployment_id = ?params.deployment_id,
            deployment_hash = %deployment_hash,
            hours = hours,
            "Queued error summary command via MCP"
        );

        Ok(ToolContent::Text {
            text: result.to_string(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "get_error_summary".to_string(),
            description: "Get a summary of errors and warnings from container logs. Returns categorized error counts, most frequent errors, and suggested fixes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment_id": {
                        "type": "number",
                        "description": "The deployment/installation ID (for legacy User Service deployments)"
                    },
                    "deployment_hash": {
                        "type": "string",
                        "description": "The deployment hash (for Stack Builder deployments). Use this if available in context."
                    },
                    "app_code": {
                        "type": "string",
                        "description": "Specific app/container to analyze. If omitted, analyzes all containers."
                    },
                    "hours": {
                        "type": "number",
                        "description": "Number of hours to look back (default: 24, max: 168)"
                    }
                },
                "required": []
            }),
        }
    }
}
