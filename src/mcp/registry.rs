use crate::configuration::Settings;
use crate::models;
use actix_web::web;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

use super::protocol::{Tool, ToolContent};
use crate::mcp::tools::{
    AddCloudTool,
    ApplyVaultConfigTool,
    CancelDeploymentTool,
    CloneProjectTool,
    ConfigureProxyTool,
    CreateProjectAppTool,
    CreateProjectTool,
    DeleteAppEnvVarTool,
    DeleteCloudTool,
    DeleteProjectTool,
    DeleteProxyTool,
    DiagnoseDeploymentTool,
    EscalateToSupportTool,
    GetAppConfigTool,
    // Phase 5: App Configuration tools
    GetAppEnvVarsTool,
    GetCloudTool,
    GetContainerHealthTool,
    GetContainerLogsTool,
    GetDeploymentStatusTool,
    GetErrorSummaryTool,
    GetInstallationDetailsTool,
    GetLiveChatInfoTool,
    GetProjectTool,
    GetSubscriptionPlanTool,
    GetUserProfileTool,
    // Phase 5: Vault Configuration tools
    GetVaultConfigTool,
    ListCloudsTool,
    ListInstallationsTool,
    ListProjectsTool,
    ListProxiesTool,
    ListTemplatesTool,
    ListVaultConfigsTool,
    RestartContainerTool,
    SearchApplicationsTool,
    SetAppEnvVarTool,
    SetVaultConfigTool,
    StartContainerTool,
    StartDeploymentTool,
    // Phase 5: Container Operations tools
    StopContainerTool,
    SuggestResourcesTool,
    UpdateAppDomainTool,
    UpdateAppPortsTool,
    ValidateDomainTool,
    // Phase 5: Stack Validation tool
    ValidateStackConfigTool,
};

/// Context passed to tool handlers
pub struct ToolContext {
    pub user: Arc<models::User>,
    pub pg_pool: PgPool,
    pub settings: web::Data<Settings>,
}

/// Trait for tool handlers
#[async_trait]
pub trait ToolHandler: Send + Sync {
    /// Execute the tool with given arguments
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String>;

    /// Return the tool schema definition
    fn schema(&self) -> Tool;
}

/// Tool registry managing all available MCP tools
pub struct ToolRegistry {
    handlers: HashMap<String, Box<dyn ToolHandler>>,
}

impl ToolRegistry {
    /// Create a new tool registry with all handlers registered
    pub fn new() -> Self {
        let mut registry = Self {
            handlers: HashMap::new(),
        };

        // Project management tools
        registry.register("list_projects", Box::new(ListProjectsTool));
        registry.register("get_project", Box::new(GetProjectTool));
        registry.register("create_project", Box::new(CreateProjectTool));
        registry.register("create_project_app", Box::new(CreateProjectAppTool));

        // Template & discovery tools
        registry.register("suggest_resources", Box::new(SuggestResourcesTool));
        registry.register("list_templates", Box::new(ListTemplatesTool));
        registry.register("validate_domain", Box::new(ValidateDomainTool));

        // Phase 3: Deployment tools
        registry.register("get_deployment_status", Box::new(GetDeploymentStatusTool));
        registry.register("start_deployment", Box::new(StartDeploymentTool));
        registry.register("cancel_deployment", Box::new(CancelDeploymentTool));

        // Phase 3: Cloud tools
        registry.register("list_clouds", Box::new(ListCloudsTool));
        registry.register("get_cloud", Box::new(GetCloudTool));
        registry.register("add_cloud", Box::new(AddCloudTool));
        registry.register("delete_cloud", Box::new(DeleteCloudTool));

        // Phase 3: Project management
        registry.register("delete_project", Box::new(DeleteProjectTool));
        registry.register("clone_project", Box::new(CloneProjectTool));

        // Phase 4: User & Account tools (AI Integration)
        registry.register("get_user_profile", Box::new(GetUserProfileTool));
        registry.register("get_subscription_plan", Box::new(GetSubscriptionPlanTool));
        registry.register("list_installations", Box::new(ListInstallationsTool));
        registry.register(
            "get_installation_details",
            Box::new(GetInstallationDetailsTool),
        );
        registry.register("search_applications", Box::new(SearchApplicationsTool));

        // Phase 4: Monitoring & Logs tools (AI Integration)
        registry.register("get_container_logs", Box::new(GetContainerLogsTool));
        registry.register("get_container_health", Box::new(GetContainerHealthTool));
        registry.register("restart_container", Box::new(RestartContainerTool));
        registry.register("diagnose_deployment", Box::new(DiagnoseDeploymentTool));

        // Phase 4: Support & Escalation tools (AI Integration)
        registry.register("escalate_to_support", Box::new(EscalateToSupportTool));
        registry.register("get_live_chat_info", Box::new(GetLiveChatInfoTool));

        // Phase 5: Container Operations tools (Agent-Based Deployment)
        registry.register("stop_container", Box::new(StopContainerTool));
        registry.register("start_container", Box::new(StartContainerTool));
        registry.register("get_error_summary", Box::new(GetErrorSummaryTool));

        // Phase 5: App Configuration Management tools
        registry.register("get_app_env_vars", Box::new(GetAppEnvVarsTool));
        registry.register("set_app_env_var", Box::new(SetAppEnvVarTool));
        registry.register("delete_app_env_var", Box::new(DeleteAppEnvVarTool));
        registry.register("get_app_config", Box::new(GetAppConfigTool));
        registry.register("update_app_ports", Box::new(UpdateAppPortsTool));
        registry.register("update_app_domain", Box::new(UpdateAppDomainTool));

        // Phase 5: Stack Validation tool
        registry.register("validate_stack_config", Box::new(ValidateStackConfigTool));

        // Phase 5: Vault Configuration tools
        registry.register("get_vault_config", Box::new(GetVaultConfigTool));
        registry.register("set_vault_config", Box::new(SetVaultConfigTool));
        registry.register("list_vault_configs", Box::new(ListVaultConfigsTool));
        registry.register("apply_vault_config", Box::new(ApplyVaultConfigTool));

        // Phase 6: Proxy Management tools (Nginx Proxy Manager)
        registry.register("configure_proxy", Box::new(ConfigureProxyTool));
        registry.register("delete_proxy", Box::new(DeleteProxyTool));
        registry.register("list_proxies", Box::new(ListProxiesTool));

        registry
    }

    /// Register a tool handler
    pub fn register(&mut self, name: &str, handler: Box<dyn ToolHandler>) {
        self.handlers.insert(name.to_string(), handler);
    }

    /// Get a tool handler by name
    pub fn get(&self, name: &str) -> Option<&Box<dyn ToolHandler>> {
        self.handlers.get(name)
    }

    /// List all available tools
    pub fn list_tools(&self) -> Vec<Tool> {
        self.handlers.values().map(|h| h.schema()).collect()
    }

    /// Check if a tool exists
    pub fn has_tool(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Get count of registered tools
    pub fn count(&self) -> usize {
        self.handlers.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
