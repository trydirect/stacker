//! MCP Tools for User Service integration.
//!
//! These tools provide AI access to:
//! - User profile information
//! - Subscription plans and limits
//! - Installations/deployments list
//! - Application catalog

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::connectors::user_service::UserServiceClient;
use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};
use serde::Deserialize;

/// Get current user's profile information
pub struct GetUserProfileTool;

#[async_trait]
impl ToolHandler for GetUserProfileTool {
    async fn execute(&self, _args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let client = UserServiceClient::new_public(&context.settings.user_service_url);

        // Use the user's token from context to call User Service
        let token = context.user.access_token.as_deref().unwrap_or("");

        let profile = client
            .get_user_profile(token)
            .await
            .map_err(|e| format!("Failed to fetch user profile: {}", e))?;

        let result =
            serde_json::to_string(&profile).map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!(user_id = %context.user.id, "Fetched user profile via MCP");

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "get_user_profile".to_string(),
            description:
                "Get the current user's profile information including email, name, and roles"
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }
}

/// Get user's subscription plan and limits
pub struct GetSubscriptionPlanTool;

#[async_trait]
impl ToolHandler for GetSubscriptionPlanTool {
    async fn execute(&self, _args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let client = UserServiceClient::new_public(&context.settings.user_service_url);
        let token = context.user.access_token.as_deref().unwrap_or("");

        let plan = client
            .get_subscription_plan(token)
            .await
            .map_err(|e| format!("Failed to fetch subscription plan: {}", e))?;

        let result =
            serde_json::to_string(&plan).map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!(user_id = %context.user.id, "Fetched subscription plan via MCP");

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "get_subscription_plan".to_string(),
            description: "Get the user's current subscription plan including limits (max deployments, apps per deployment, storage, bandwidth) and features".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }
}

/// List user's installations (deployments)
pub struct ListInstallationsTool;

#[async_trait]
impl ToolHandler for ListInstallationsTool {
    async fn execute(&self, _args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let client = UserServiceClient::new_public(&context.settings.user_service_url);
        let token = context.user.access_token.as_deref().unwrap_or("");

        let installations = client
            .list_installations(token)
            .await
            .map_err(|e| format!("Failed to fetch installations: {}", e))?;

        let result = serde_json::to_string(&installations)
            .map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!(
            user_id = %context.user.id,
            count = installations.len(),
            "Listed installations via MCP"
        );

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "list_installations".to_string(),
            description: "List all user's deployments/installations with their status, cloud provider, and domain".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }
}

/// Get specific installation details
pub struct GetInstallationDetailsTool;

#[async_trait]
impl ToolHandler for GetInstallationDetailsTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            installation_id: i64,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        let client = UserServiceClient::new_public(&context.settings.user_service_url);
        let token = context.user.access_token.as_deref().unwrap_or("");

        let installation = client
            .get_installation(token, params.installation_id)
            .await
            .map_err(|e| format!("Failed to fetch installation details: {}", e))?;

        let result = serde_json::to_string(&installation)
            .map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!(
            user_id = %context.user.id,
            installation_id = params.installation_id,
            "Fetched installation details via MCP"
        );

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "get_installation_details".to_string(),
            description: "Get detailed information about a specific deployment/installation including apps, server IP, and agent configuration".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "installation_id": {
                        "type": "number",
                        "description": "The installation/deployment ID to fetch details for"
                    }
                },
                "required": ["installation_id"]
            }),
        }
    }
}

/// Search available applications in the catalog
pub struct SearchApplicationsTool;

#[async_trait]
impl ToolHandler for SearchApplicationsTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            query: Option<String>,
        }

        let params: Args =
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

        let client = UserServiceClient::new_public(&context.settings.user_service_url);
        let token = context.user.access_token.as_deref().unwrap_or("");

        let applications = client
            .search_applications(token, params.query.as_deref())
            .await
            .map_err(|e| format!("Failed to search applications: {}", e))?;

        let result = serde_json::to_string(&applications)
            .map_err(|e| format!("Serialization error: {}", e))?;

        tracing::info!(
            user_id = %context.user.id,
            query = ?params.query,
            count = applications.len(),
            "Searched applications via MCP"
        );

        Ok(ToolContent::Text { text: result })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "search_applications".to_string(),
            description: "Search available applications/services in the catalog that can be added to a stack. Returns app details including Docker image, default port, and description.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Optional search query to filter applications by name"
                    }
                },
                "required": []
            }),
        }
    }
}
