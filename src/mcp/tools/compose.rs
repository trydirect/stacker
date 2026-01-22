use async_trait::async_trait;
use serde_json::{json, Value};

use crate::db;
use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};
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

        let args: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

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

        tracing::info!(
            "Deleted project {} for user {}",
            args.project_id,
            context.user.id
        );

        Ok(ToolContent::Text {
            text: response.to_string(),
        })
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

        let args: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

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

        tracing::info!(
            "Cloned project {} to {} for user {}",
            args.project_id,
            cloned_project.id,
            context.user.id
        );

        Ok(ToolContent::Text {
            text: response.to_string(),
        })
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

/// Validate a project's stack configuration before deployment
pub struct ValidateStackConfigTool;

#[async_trait]
impl ToolHandler for ValidateStackConfigTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            project_id: i32,
        }

        let args: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        // Fetch project
        let project = db::project::fetch(&context.pg_pool, args.project_id)
            .await
            .map_err(|e| format!("Project not found: {}", e))?
            .ok_or_else(|| "Project not found".to_string())?;

        // Check ownership
        if project.user_id != context.user.id {
            return Err("Project not found".to_string());
        }

        // Fetch all apps in the project
        let apps = db::project_app::fetch_by_project(&context.pg_pool, args.project_id)
            .await
            .map_err(|e| format!("Failed to fetch project apps: {}", e))?;

        let mut errors: Vec<Value> = Vec::new();
        let mut warnings: Vec<Value> = Vec::new();
        let mut info: Vec<Value> = Vec::new();

        // Validation checks
        
        // 1. Check if project has any apps
        if apps.is_empty() {
            errors.push(json!({
                "code": "NO_APPS",
                "message": "Project has no applications configured. Add at least one app to deploy.",
                "severity": "error"
            }));
        }

        // 2. Check each app for required configuration
        let mut used_ports: std::collections::HashMap<u16, String> = std::collections::HashMap::new();
        let mut has_web_app = false;

        for app in &apps {
            let app_code = &app.code;
            
            // Check for image
            if app.image.is_empty() {
                errors.push(json!({
                    "code": "MISSING_IMAGE",
                    "app": app_code,
                    "message": format!("App '{}' has no Docker image configured.", app_code),
                    "severity": "error"
                }));
            }

            // Check for port conflicts
            if let Some(ports) = &app.ports {
                if let Some(ports_array) = ports.as_array() {
                    for port_config in ports_array {
                        if let Some(host_port) = port_config.get("host").and_then(|v| v.as_u64()) {
                            let host_port = host_port as u16;
                            if let Some(existing_app) = used_ports.get(&host_port) {
                                errors.push(json!({
                                    "code": "PORT_CONFLICT",
                                    "app": app_code,
                                    "port": host_port,
                                    "message": format!("Port {} is used by both '{}' and '{}'.", host_port, existing_app, app_code),
                                    "severity": "error"
                                }));
                            } else {
                                used_ports.insert(host_port, app_code.to_string());
                            }
                            
                            // Check for common ports
                            if host_port == 80 || host_port == 443 {
                                has_web_app = true;
                            }
                        }
                    }
                }
            }

            // Check for common misconfigurations
            if let Some(env) = &app.environment {
                if let Some(env_obj) = env.as_object() {
                    // PostgreSQL specific checks
                    if app_code.contains("postgres") || app.image.contains("postgres") {
                        if !env_obj.contains_key("POSTGRES_PASSWORD") && !env_obj.contains_key("POSTGRES_HOST_AUTH_METHOD") {
                            warnings.push(json!({
                                "code": "MISSING_DB_PASSWORD",
                                "app": app_code,
                                "message": "PostgreSQL requires POSTGRES_PASSWORD or POSTGRES_HOST_AUTH_METHOD environment variable.",
                                "severity": "warning",
                                "suggestion": "Set POSTGRES_PASSWORD to a secure value."
                            }));
                        }
                    }

                    // MySQL/MariaDB specific checks
                    if app_code.contains("mysql") || app_code.contains("mariadb") {
                        if !env_obj.contains_key("MYSQL_ROOT_PASSWORD") && !env_obj.contains_key("MYSQL_ALLOW_EMPTY_PASSWORD") {
                            warnings.push(json!({
                                "code": "MISSING_DB_PASSWORD",
                                "app": app_code,
                                "message": "MySQL/MariaDB requires MYSQL_ROOT_PASSWORD environment variable.",
                                "severity": "warning",
                                "suggestion": "Set MYSQL_ROOT_PASSWORD to a secure value."
                            }));
                        }
                    }
                }
            }

            // Check for domain configuration on web apps
            if (app_code.contains("nginx") || app_code.contains("apache") || app_code.contains("traefik")) 
                && app.domain.is_none() {
                info.push(json!({
                    "code": "NO_DOMAIN",
                    "app": app_code,
                    "message": format!("Web server '{}' has no domain configured. It will only be accessible via IP address.", app_code),
                    "severity": "info"
                }));
            }
        }

        // 3. Check for recommended practices
        if !has_web_app && !apps.is_empty() {
            info.push(json!({
                "code": "NO_WEB_PORT",
                "message": "No application is configured on port 80 or 443. The stack may not be accessible from a web browser.",
                "severity": "info"
            }));
        }

        // Build validation result
        let is_valid = errors.is_empty();
        let result = json!({
            "project_id": args.project_id,
            "project_name": project.name,
            "is_valid": is_valid,
            "apps_count": apps.len(),
            "errors": errors,
            "warnings": warnings,
            "info": info,
            "summary": {
                "error_count": errors.len(),
                "warning_count": warnings.len(),
                "info_count": info.len()
            },
            "recommendation": if is_valid {
                if warnings.is_empty() {
                    "Stack configuration looks good! Ready for deployment.".to_string()
                } else {
                    format!("Stack can be deployed but has {} warning(s) to review.", warnings.len())
                }
            } else {
                format!("Stack has {} error(s) that must be fixed before deployment.", errors.len())
            }
        });

        tracing::info!(
            user_id = %context.user.id,
            project_id = args.project_id,
            is_valid = is_valid,
            errors = errors.len(),
            warnings = warnings.len(),
            "Validated stack configuration via MCP"
        );

        Ok(ToolContent::Text {
            text: serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "validate_stack_config".to_string(),
            description: "Validate a project's stack configuration before deployment. Checks for missing images, port conflicts, required environment variables, and other common issues.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "number",
                        "description": "Project ID to validate"
                    }
                },
                "required": ["project_id"]
            }),
        }
    }
}
