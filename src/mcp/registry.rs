use crate::configuration::Settings;
use actix_web::web;
use crate::models;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

use super::protocol::{Tool, ToolContent};

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
        let registry = Self {
            handlers: HashMap::new(),
        };

        // TODO: Register tools as they are implemented
        // registry.register("create_project", Box::new(CreateProjectTool));
        // registry.register("list_projects", Box::new(ListProjectsTool));
        // registry.register("get_project", Box::new(GetProjectTool));
        // registry.register("suggest_resources", Box::new(SuggestResourcesTool));

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
