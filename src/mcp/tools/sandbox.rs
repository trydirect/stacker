//! MCP tools for sandboxes (WS-S S4): create, exec, file ops, kill, list.

use crate::db;
use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};
use crate::models;
use crate::services::sandbox as sandbox_service;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

/// Load a sandbox the caller owns; `None` when missing or not theirs.
async fn owned_sandbox(context: &ToolContext, id: Uuid) -> Result<models::Sandbox, String> {
    let sandbox = db::sandbox::fetch(&context.pg_pool, id)
        .await
        .map_err(|e| format!("Failed to fetch sandbox: {}", e))?
        .ok_or_else(|| "Sandbox not found".to_string())?;
    if sandbox.user_id != context.user.id {
        return Err("Sandbox not found".to_string());
    }
    Ok(sandbox)
}

fn deployment_id(sandbox: &models::Sandbox) -> Result<i32, String> {
    sandbox
        .deployment_id
        .ok_or_else(|| "Sandbox has no target deployment".to_string())
}

fn sandbox_json(s: &models::Sandbox) -> Value {
    serde_json::to_value(models::SandboxResponse::from(s)).unwrap_or(Value::Null)
}

pub struct ListSandboxesTool;

#[async_trait]
impl ToolHandler for ListSandboxesTool {
    async fn execute(&self, _args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let list = db::sandbox::list_by_user(&context.pg_pool, &context.user.id)
            .await
            .map_err(|e| format!("Failed to list sandboxes: {}", e))?;
        let items: Vec<Value> = list.iter().map(sandbox_json).collect();
        Ok(ToolContent::Text {
            text: serde_json::to_string_pretty(&items).unwrap_or_default(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "sandbox_list".to_string(),
            description: "List the caller's sandboxes (ephemeral containers for untrusted/AI code)"
                .to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        }
    }
}

#[derive(Deserialize)]
struct CreateArgs {
    #[serde(rename = "projectId")]
    project_id: Option<i32>,
    image: String,
    #[serde(default)]
    cpu: Option<f64>,
    #[serde(rename = "memMb")]
    mem_mb: Option<i32>,
}

pub struct CreateSandboxTool;

#[async_trait]
impl ToolHandler for CreateSandboxTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let params: CreateArgs =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;
        let deployment_id = match params.project_id {
            Some(project_id) => db::deployment::fetch_by_project_id(&context.pg_pool, project_id)
                .await
                .map_err(|e| format!("Failed to resolve deployment: {}", e))?
                .map(|d| d.id),
            None => None,
        };
        let sandbox = db::sandbox::insert(
            &context.pg_pool,
            &context.user.id,
            params.project_id,
            deployment_id,
            &params.image,
            "runc",
            "isolated",
            params.cpu.unwrap_or(1.0),
            params.mem_mb.unwrap_or(512),
            128,
            1800,
            86400,
            None,
        )
        .await
        .map_err(|e| format!("Failed to create sandbox: {}", e))?;
        Ok(ToolContent::Text {
            text: serde_json::to_string_pretty(&sandbox_json(&sandbox)).unwrap_or_default(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "sandbox_create".to_string(),
            description: "Create an isolated ephemeral sandbox (create -> exec -> kill)"
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "projectId": { "type": "integer" },
                    "image": { "type": "string" },
                    "cpu": { "type": "number" },
                    "memMb": { "type": "integer" }
                },
                "required": ["image"]
            }),
        }
    }
}

#[derive(Deserialize)]
struct ExecArgs {
    id: String,
    command: String,
    #[serde(rename = "timeoutSec")]
    timeout_sec: Option<u32>,
}

pub struct ExecSandboxTool;

#[async_trait]
impl ToolHandler for ExecSandboxTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let params: ExecArgs =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;
        let id = Uuid::parse_str(&params.id).map_err(|e| format!("Invalid sandbox id: {}", e))?;
        let sandbox = owned_sandbox(context, id).await?;
        let deployment_id = deployment_id(&sandbox)?;
        let result = sandbox_service::exec(
            &context.pg_pool,
            id,
            deployment_id,
            &params.command,
            params.timeout_sec,
        )
        .await?;
        Ok(ToolContent::Text {
            text: serde_json::to_string_pretty(&result).unwrap_or_default(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "sandbox_exec".to_string(),
            description:
                "Run a shell command inside a sandbox (per-call timeout, 1 MiB output cap)"
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "command": { "type": "string" },
                    "timeoutSec": { "type": "integer" }
                },
                "required": ["id", "command"]
            }),
        }
    }
}

#[derive(Deserialize)]
struct FileArgs {
    id: String,
    path: String,
    #[serde(rename = "contentBase64")]
    content_base64: Option<String>,
}

pub struct WriteSandboxFileTool;

#[async_trait]
impl ToolHandler for WriteSandboxFileTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let params: FileArgs =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;
        let id = Uuid::parse_str(&params.id).map_err(|e| format!("Invalid sandbox id: {}", e))?;
        let sandbox = owned_sandbox(context, id).await?;
        let deployment_id = deployment_id(&sandbox)?;
        let content = params.content_base64.unwrap_or_default();
        let result = sandbox_service::write_file(
            &context.pg_pool,
            id,
            deployment_id,
            &params.path,
            &content,
        )
        .await?;
        Ok(ToolContent::Text {
            text: serde_json::to_string_pretty(&result).unwrap_or_default(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "sandbox_write_file".to_string(),
            description: "Write a base64 file into a sandbox (10 MiB cap)".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "path": { "type": "string" },
                    "contentBase64": { "type": "string" }
                },
                "required": ["id", "path", "contentBase64"]
            }),
        }
    }
}

pub struct ReadSandboxFileTool;

#[async_trait]
impl ToolHandler for ReadSandboxFileTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let params: FileArgs =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;
        let id = Uuid::parse_str(&params.id).map_err(|e| format!("Invalid sandbox id: {}", e))?;
        let sandbox = owned_sandbox(context, id).await?;
        let deployment_id = deployment_id(&sandbox)?;
        let result =
            sandbox_service::read_file(&context.pg_pool, id, deployment_id, &params.path).await?;
        Ok(ToolContent::Text {
            text: serde_json::to_string_pretty(&result).unwrap_or_default(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "sandbox_read_file".to_string(),
            description: "Read a file from a sandbox (returns base64, 10 MiB cap)".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["id", "path"]
            }),
        }
    }
}

#[derive(Deserialize)]
struct KillArgs {
    id: String,
}

pub struct KillSandboxTool;

#[async_trait]
impl ToolHandler for KillSandboxTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let params: KillArgs =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;
        let id = Uuid::parse_str(&params.id).map_err(|e| format!("Invalid sandbox id: {}", e))?;
        let sandbox = owned_sandbox(context, id).await?;
        if let Some(deployment_id) = sandbox.deployment_id {
            let command_params = json!({ "action": "kill", "sandboxId": id });
            let _ = crate::services::agent_dispatcher::AgentDispatcher::new(&context.pg_pool)
                .queue_command(deployment_id, "sandbox", command_params)
                .await;
        }
        db::sandbox::update_status(&context.pg_pool, id, "killed")
            .await
            .map_err(|e| format!("Failed to mark sandbox killed: {}", e))?;
        Ok(ToolContent::Text {
            text: json!({ "killed": true, "id": id }).to_string(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "sandbox_kill".to_string(),
            description: "Kill a sandbox (idempotent)".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }),
        }
    }
}
