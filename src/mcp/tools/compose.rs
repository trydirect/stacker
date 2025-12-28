use async_trait::async_trait;
use serde_json::{json, Value};

use crate::db;
use crate::mcp::registry::{ToolContext, ToolHandler};
use crate::mcp::protocol::{Tool, ToolContent};
use serde::Deserialize;

/// Delete a project
pub struct DeleteProjectTool;

#[async_trait]
impl ToolHandler for DeleteProjectTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            project_id: i32,
        }

        let args: Args = serde_json::from_value(args)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let project = db::project::fetch(&context.pg_pool, args.project_id)
            .await
            .map_err(|e| format!("Project not found: {}", e))?
            .ok_or_else(|| "Project not found".to_string())?;

        if project.user_id != context.user.id {
            return Err("Unauthorized: You do not own this project".to_string());
        }

        db::project::delete(&context.pg_pool, args.project_id)
            .await
            .map_err(|e| format!("Failed to delete project: {}", e))?;

        let response = serde_json::json!({
            "project_id": args.project_id,
            "message": "Project deleted successfully"
        });

        tracing::info!("Deleted project {} for user {}", args.project_id, context.user.id);

        Ok(ToolContent::Text { text: response.to_string() })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "delete_project".to_string(),
            description: "Delete a project permanently".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "number",
                        "description": "Project ID to delete"
                    }
                },
                "required": ["project_id"]
            }),
        }
    }
}

/// Clone a project
pub struct CloneProjectTool;

#[async_trait]
impl ToolHandler for CloneProjectTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            project_id: i32,
            new_name: String,
        }

        let args: Args = serde_json::from_value(args)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        if args.new_name.trim().is_empty() {
            return Err("New project name cannot be empty".to_string());
        }

        if args.new_name.len() > 255 {
            return Err("Project name must be 255 characters or less".to_string());
        }

        let project = db::project::fetch(&context.pg_pool, args.project_id)
            .await
            .map_err(|e| format!("Project not found: {}", e))?
            .ok_or_else(|| "Project not found".to_string())?;

        if project.user_id != context.user.id {
            return Err("Unauthorized: You do not own this project".to_string());
        }

        // Create new project with cloned data
        let cloned_project = crate::models::Project::new(
            context.user.id.clone(),
            args.new_name.clone(),
            project.metadata.clone(),
            project.request_json.clone(),
        );

        let cloned_project = db::project::insert(&context.pg_pool, cloned_project)
            .await
            .map_err(|e| format!("Failed to clone project: {}", e))?;

        let response = serde_json::json!({
            "original_id": args.project_id,
            "cloned_id": cloned_project.id,
            "cloned_name": cloned_project.name,
            "message": "Project cloned successfully"
        });

        tracing::info!("Cloned project {} to {} for user {}", args.project_id, cloned_project.id, context.user.id);

        Ok(ToolContent::Text { text: response.to_string() })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "clone_project".to_string(),
            description: "Clone/duplicate an existing project with a new name".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "number",
                        "description": "Project ID to clone"
                    },
                    "new_name": {
                        "type": "string",
                        "description": "Name for the cloned project (max 255 chars)"
                    }
                },
                "required": ["project_id", "new_name"]
            }),
        }
    }
}
