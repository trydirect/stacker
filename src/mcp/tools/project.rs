use async_trait::async_trait;
use serde_json::{json, Value};

use crate::db;
use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};
use serde::Deserialize;

/// List user's projects
pub struct ListProjectsTool;

#[async_trait]
impl ToolHandler for ListProjectsTool {
    async fn execute(&self, _args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let projects = db::project::fetch_by_user(&context.pg_pool, &context.user.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch projects: {}", e);
                format!("Database error: {}", e)
            })?;

        let result =
            serde_json::to_string(&projects).map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!(
            "Listed {} projects for user {}",
            projects.len(),
            context.user.id
        );

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "list_projects".to_string(),
            description: "List all projects owned by the authenticated user".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }
}

/// Get a specific project by ID
pub struct GetProjectTool;

#[async_trait]
impl ToolHandler for GetProjectTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            id: i32,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        let project = db::project::fetch(&context.pg_pool, params.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch project {}: {}", params.id, e);
                format!("Database error: {}", e)
            })?;

        let result =
            serde_json::to_string(&project).map_err(|e| format!("Serialization error: {}", e))?;

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "get_project".to_string(),
            description: "Get details of a specific project by ID".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "number",
                        "description": "Project ID"
                    }
                },
                "required": ["id"]
            }),
        }
    }
}

/// Create a new project
pub struct CreateProjectTool;

#[async_trait]
impl ToolHandler for CreateProjectTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct CreateArgs {
            name: String,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            apps: Vec<Value>,
        }

        let params: CreateArgs =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        if params.name.trim().is_empty() {
            return Err("Project name cannot be empty".to_string());
        }

        if params.name.len() > 255 {
            return Err("Project name too long (max 255 characters)".to_string());
        }

        // Create a new Project model with empty metadata/request
        let project = crate::models::Project::new(
            context.user.id.clone(),
            params.name.clone(),
            serde_json::json!({}),
            serde_json::json!(params.apps),
        );

        let project = db::project::insert(&context.pg_pool, project)
            .await
            .map_err(|e| {
                tracing::error!("Failed to create project: {}", e);
                format!("Failed to create project: {}", e)
            })?;

        let result =
            serde_json::to_string(&project).map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!(
            "Created project {} for user {}",
            project.id,
            context.user.id
        );

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "create_project".to_string(),
            description: "Create a new application stack project with services and configuration"
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Project name (required, max 255 chars)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Project description (optional)"
                    },
                    "apps": {
                        "type": "array",
                        "description": "List of applications/services to include",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {
                                    "type": "string",
                                    "description": "Service name"
                                },
                                "dockerImage": {
                                    "type": "object",
                                    "properties": {
                                        "namespace": { "type": "string" },
                                        "repository": {
                                            "type": "string",
                                            "description": "Docker image repository"
                                        },
                                        "tag": { "type": "string" }
                                    },
                                    "required": ["repository"]
                                }
                            }
                        }
                    }
                },
                "required": ["name"]
            }),
        }
    }
}
