use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::types::uuid::Uuid;
use sqlx::types::JsonValue;

/// Command status enum matching the database CHECK constraint
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum CommandStatus {
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "sent")]
    Sent,
    #[serde(rename = "executing")]
    Executing,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
}

impl std::fmt::Display for CommandStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandStatus::Queued => write!(f, "queued"),
            CommandStatus::Sent => write!(f, "sent"),
            CommandStatus::Executing => write!(f, "executing"),
            CommandStatus::Completed => write!(f, "completed"),
            CommandStatus::Failed => write!(f, "failed"),
            CommandStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Command priority enum matching the database CHECK constraint
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum CommandPriority {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "critical")]
    Critical,
}

impl CommandPriority {
    /// Convert priority to integer for queue ordering
    pub fn to_int(&self) -> i32 {
        match self {
            CommandPriority::Low => 0,
            CommandPriority::Normal => 1,
            CommandPriority::High => 2,
            CommandPriority::Critical => 3,
        }
    }
}

impl std::fmt::Display for CommandPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandPriority::Low => write!(f, "low"),
            CommandPriority::Normal => write!(f, "normal"),
            CommandPriority::High => write!(f, "high"),
            CommandPriority::Critical => write!(f, "critical"),
        }
    }
}

/// Command model representing a command to be executed on an agent
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, Default)]
pub struct Command {
    pub id: Uuid,
    pub command_id: String,
    pub deployment_hash: String,
    pub r#type: String,
    pub status: String,
    pub priority: String,
    pub parameters: Option<JsonValue>,
    pub result: Option<JsonValue>,
    pub error: Option<JsonValue>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub timeout_seconds: Option<i32>,
    pub metadata: Option<JsonValue>,
}

impl Command {
    /// Create a new command with defaults
    pub fn new(
        command_id: String,
        deployment_hash: String,
        command_type: String,
        created_by: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            command_id,
            deployment_hash,
            r#type: command_type,
            status: CommandStatus::Queued.to_string(),
            priority: CommandPriority::Normal.to_string(),
            parameters: None,
            result: None,
            error: None,
            created_by,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            timeout_seconds: Some(300), // Default 5 minutes
            metadata: None,
        }
    }

    /// Builder: Set priority
    pub fn with_priority(mut self, priority: CommandPriority) -> Self {
        self.priority = priority.to_string();
        self
    }

    /// Builder: Set parameters
    pub fn with_parameters(mut self, parameters: JsonValue) -> Self {
        self.parameters = Some(parameters);
        self
    }

    /// Builder: Set timeout in seconds
    pub fn with_timeout(mut self, seconds: i32) -> Self {
        self.timeout_seconds = Some(seconds);
        self
    }

    /// Builder: Set metadata
    pub fn with_metadata(mut self, metadata: JsonValue) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Mark command as sent
    pub fn mark_sent(mut self) -> Self {
        self.status = CommandStatus::Sent.to_string();
        self.updated_at = Utc::now();
        self
    }

    /// Mark command as executing
    pub fn mark_executing(mut self) -> Self {
        self.status = CommandStatus::Executing.to_string();
        self.updated_at = Utc::now();
        self
    }

    /// Mark command as completed
    pub fn mark_completed(mut self) -> Self {
        self.status = CommandStatus::Completed.to_string();
        self.updated_at = Utc::now();
        self
    }

    /// Mark command as failed
    pub fn mark_failed(mut self) -> Self {
        self.status = CommandStatus::Failed.to_string();
        self.updated_at = Utc::now();
        self
    }

    /// Mark command as cancelled
    pub fn mark_cancelled(mut self) -> Self {
        self.status = CommandStatus::Cancelled.to_string();
        self.updated_at = Utc::now();
        self
    }
}

/// Command result payload from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub command_id: String,
    pub deployment_hash: String,
    pub status: CommandStatus,
    pub result: Option<JsonValue>,
    pub error: Option<CommandError>,
    pub metadata: Option<JsonValue>,
}

/// Command error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
    pub details: Option<JsonValue>,
}

/// Command queue entry for efficient polling
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CommandQueueEntry {
    pub command_id: String,
    pub deployment_hash: String,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
}
