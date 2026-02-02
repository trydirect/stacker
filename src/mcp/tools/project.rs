use async_trait::async_trait;
use serde_json::{json, Value};

use crate::db;
use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};
use crate::services::ProjectAppService;
use serde::Deserialize;
use std::sync::Arc;

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

/// Create or update an app in a project (custom service)
pub struct CreateProjectAppTool;

#[async_trait]
impl ToolHandler for CreateProjectAppTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            project_id: i32,
            #[serde(alias = "app_code")]
            code: String,
            image: String,
            #[serde(default)]
            name: Option<String>,
            #[serde(default, alias = "environment")]
            env: Option<Value>,
            #[serde(default)]
            ports: Option<Value>,
            #[serde(default)]
            volumes: Option<Value>,
            #[serde(default)]
            config_files: Option<Value>,
            #[serde(default)]
            domain: Option<String>,
            #[serde(default)]
            ssl_enabled: Option<bool>,
            #[serde(default)]
            resources: Option<Value>,
            #[serde(default)]
            restart_policy: Option<String>,
            #[serde(default)]
            command: Option<String>,
            #[serde(default)]
            entrypoint: Option<String>,
            #[serde(default)]
            networks: Option<Value>,
            #[serde(default)]
            depends_on: Option<Value>,
            #[serde(default)]
            healthcheck: Option<Value>,
            #[serde(default)]
            labels: Option<Value>,
            #[serde(default)]
            enabled: Option<bool>,
            #[serde(default)]
            deploy_order: Option<i32>,
            #[serde(default)]
            deployment_hash: Option<String>,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        if params.code.trim().is_empty() {
            return Err("app code is required".to_string());
        }

        if params.image.trim().is_empty() {
            return Err("image is required".to_string());
        }

        let project = db::project::fetch(&context.pg_pool, params.project_id)
            .await
            .map_err(|e| format!("Database error: {}", e))?
            .ok_or_else(|| "Project not found".to_string())?;

        if project.user_id != context.user.id {
            return Err("Project not found".to_string());
        }

        let mut app = crate::models::ProjectApp::default();
        app.project_id = params.project_id;
        app.code = params.code.trim().to_string();
        app.name = params
            .name
            .clone()
            .unwrap_or_else(|| params.code.trim().to_string());
        app.image = params.image.trim().to_string();
        app.environment = params.env.clone();
        app.ports = params.ports.clone();
        app.volumes = params.volumes.clone();
        app.domain = params.domain.clone();
        app.ssl_enabled = params.ssl_enabled;
        app.resources = params.resources.clone();
        app.restart_policy = params.restart_policy.clone();
        app.command = params.command.clone();
        app.entrypoint = params.entrypoint.clone();
        app.networks = params.networks.clone();
        app.depends_on = params.depends_on.clone();
        app.healthcheck = params.healthcheck.clone();
        app.labels = params.labels.clone();
        app.enabled = params.enabled.or(Some(true));
        app.deploy_order = params.deploy_order;

        if let Some(config_files) = params.config_files.clone() {
            let mut labels = app.labels.clone().unwrap_or(json!({}));
            if let Some(obj) = labels.as_object_mut() {
                obj.insert("config_files".to_string(), config_files);
            }
            app.labels = Some(labels);
        }

        let service = if params.deployment_hash.is_some() {
            ProjectAppService::new(Arc::new(context.pg_pool.clone()))
                .map_err(|e| format!("Failed to create app service: {}", e))?
        } else {
            ProjectAppService::new_without_sync(Arc::new(context.pg_pool.clone()))
                .map_err(|e| format!("Failed to create app service: {}", e))?
        };

        let deployment_hash = params.deployment_hash.unwrap_or_default();
        let created = service
            .upsert(&app, &project, &deployment_hash)
            .await
            .map_err(|e| format!("Failed to save app: {}", e))?;

        let result =
            serde_json::to_string(&created).map_err(|e| format!("Serialization error: {}", e))?;

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "create_project_app".to_string(),
            description: "Create or update a custom app/service within a project (writes to project_app)."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "number", "description": "Project ID" },
                    "code": { "type": "string", "description": "App code (or app_code)" },
                    "app_code": { "type": "string", "description": "Alias for code" },
                    "name": { "type": "string", "description": "Display name" },
                    "image": { "type": "string", "description": "Docker image" },
                    "env": { "type": "object", "description": "Environment variables" },
                    "ports": { "type": "array", "description": "Port mappings" },
                    "volumes": { "type": "array", "description": "Volume mounts" },
                    "config_files": { "type": "array", "description": "Additional config files" },
                    "domain": { "type": "string", "description": "Domain name" },
                    "ssl_enabled": { "type": "boolean", "description": "Enable SSL" },
                    "resources": { "type": "object", "description": "Resource limits" },
                    "restart_policy": { "type": "string", "description": "Restart policy" },
                    "command": { "type": "string", "description": "Command override" },
                    "entrypoint": { "type": "string", "description": "Entrypoint override" },
                    "networks": { "type": "array", "description": "Networks" },
                    "depends_on": { "type": "array", "description": "Dependencies" },
                    "healthcheck": { "type": "object", "description": "Healthcheck" },
                    "labels": { "type": "object", "description": "Container labels" },
                    "enabled": { "type": "boolean", "description": "Enable app" },
                    "deploy_order": { "type": "number", "description": "Deployment order" },
                    "deployment_hash": { "type": "string", "description": "Optional: sync to Vault" }
                },
                "required": ["project_id", "code", "image"]
            }),
        }
    }
}
