//! Error type shared by the agent tools. Kept small and stringly-mappable so MCP
//! `ToolHandler`s can turn it into a JSON-RPC error message.

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum AgentToolError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),

    /// The agent-supplied compose was rejected by the safety gate before any
    /// infrastructure was provisioned.
    #[error("compose rejected: {0}")]
    ComposeRejected(String),

    /// A downstream (registry, MQ, cloud, DB) operation failed.
    #[error("backend error: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, AgentToolError>;
