use crate::configuration::Settings;
use actix_web::web;
use crate::models;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

use super::protocol::{Tool, ToolContent};
use crate::mcp::tools::{
    ListProjectsTool, GetProjectTool, CreateProjectTool,
    SuggestResourcesTool, ListTemplatesTool, ValidateDomainTool,
    GetDeploymentStatusTool, StartDeploymentTool, CancelDeploymentTool,
    ListCloudsTool, GetCloudTool, AddCloudTool, DeleteCloudTool,
    DeleteProjectTool, CloneProjectTool,
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
    async fn execute(&self, args: Value, context: &ToolContext)
        -> Result<ToolContent, String>;

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
