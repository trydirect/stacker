use async_trait::async_trait;
use serde_json::{json, Value};

use crate::db;
use crate::models;
use crate::mcp::registry::{ToolContext, ToolHandler};
use crate::mcp::protocol::{Tool, ToolContent};
use serde::Deserialize;

/// List user's cloud credentials
pub struct ListCloudsTool;

#[async_trait]
impl ToolHandler for ListCloudsTool {
    async fn execute(&self, _args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let clouds = db::cloud::fetch_by_user(&context.pg_pool, &context.user.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch clouds: {}", e);
                format!("Database error: {}", e)
            })?;

        let result = serde_json::to_string(&clouds)
            .map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!("Listed {} clouds for user {}", clouds.len(), context.user.id);

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "list_clouds".to_string(),
            description: "List all cloud provider credentials owned by the authenticated user".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }
}

/// Get a specific cloud by ID
pub struct GetCloudTool;

#[async_trait]
impl ToolHandler for GetCloudTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            id: i32,
        }

        let args: Args = serde_json::from_value(args)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let cloud = db::cloud::fetch(&context.pg_pool, args.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch cloud: {}", e);
                format!("Cloud error: {}", e)
            })?
            .ok_or_else(|| "Cloud not found".to_string())?;

        let result = serde_json::to_string(&cloud)
            .map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!("Retrieved cloud {} for user {}", args.id, context.user.id);

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "get_cloud".to_string(),
            description: "Get details of a specific cloud provider credential by ID".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "number",
                        "description": "Cloud ID"
                    }
                },
                "required": ["id"]
            }),
        }
    }
}

/// Delete a cloud credential
pub struct DeleteCloudTool;

#[async_trait]
impl ToolHandler for DeleteCloudTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            id: i32,
        }

        let args: Args = serde_json::from_value(args)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let cloud = db::cloud::fetch(&context.pg_pool, args.id)
            .await
            .map_err(|e| format!("Cloud error: {}", e))?
            .ok_or_else(|| "Cloud not found".to_string())?;

        db::cloud::delete(&context.pg_pool, args.id)
            .await
            .map_err(|e| format!("Failed to delete cloud: {}", e))?;

        let response = serde_json::json!({
            "id": args.id,
            "message": "Cloud credential deleted successfully"
        });

        tracing::info!("Deleted cloud {} for user {}", args.id, context.user.id);

        Ok(ToolContent::Text { text: response.to_string() })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "delete_cloud".to_string(),
            description: "Delete a cloud provider credential".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "number",
                        "description": "Cloud ID to delete"
                    }
                },
                "required": ["id"]
            }),
        }
    }
}

/// Add new cloud credentials
pub struct AddCloudTool;

#[async_trait]
impl ToolHandler for AddCloudTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            provider: String,
            cloud_token: Option<String>,
            cloud_key: Option<String>,
            cloud_secret: Option<String>,
            save_token: Option<bool>,
        }

        let args: Args = serde_json::from_value(args)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        // Validate provider
        let valid_providers = ["aws", "digitalocean", "hetzner", "azure", "gcp"];
        if !valid_providers.contains(&args.provider.to_lowercase().as_str()) {
            return Err(format!(
                "Invalid provider. Must be one of: {}",
                valid_providers.join(", ")
            ));
        }

        // Validate at least one credential is provided
        if args.cloud_token.is_none() && args.cloud_key.is_none() && args.cloud_secret.is_none() {
            return Err("At least one of cloud_token, cloud_key, or cloud_secret must be provided".to_string());
        }

        // Create cloud record
        let cloud = models::Cloud {
            id: 0, // Will be set by DB
            user_id: context.user.id.clone(),
            provider: args.provider.clone(),
            cloud_token: args.cloud_token,
            cloud_key: args.cloud_key,
            cloud_secret: args.cloud_secret,
            save_token: args.save_token,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let created_cloud = db::cloud::insert(&context.pg_pool, cloud)
            .await
            .map_err(|e| format!("Failed to create cloud: {}", e))?;

        let response = serde_json::json!({
            "id": created_cloud.id,
            "provider": created_cloud.provider,
            "save_token": created_cloud.save_token,
            "created_at": created_cloud.created_at,
            "message": "Cloud credentials added successfully"
        });

        tracing::info!("Added cloud {} for user {}", created_cloud.id, context.user.id);

        Ok(ToolContent::Text { text: response.to_string() })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "add_cloud".to_string(),
            description: "Add new cloud provider credentials for deployments".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "provider": {
                        "type": "string",
                        "description": "Cloud provider name (aws, digitalocean, hetzner, azure, gcp)",
                        "enum": ["aws", "digitalocean", "hetzner", "azure", "gcp"]
                    },
                    "cloud_token": {
                        "type": "string",
                        "description": "Cloud API token (optional)"
                    },
                    "cloud_key": {
                        "type": "string",
                        "description": "Cloud access key (optional)"
                    },
                    "cloud_secret": {
                        "type": "string",
                        "description": "Cloud secret key (optional)"
                    },
                    "save_token": {
                        "type": "boolean",
                        "description": "Whether to save the token for future use (default: true)"
                    }
                },
                "required": ["provider"]
            }),
        }
    }
}
