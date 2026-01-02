use serde::{Deserialize, Serialize};

/// Configuration for external service connectors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub user_service: Option<UserServiceConfig>,
    pub payment_service: Option<PaymentServiceConfig>,
    pub events: Option<EventsConfig>,
}

/// User Service connector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserServiceConfig {
    /// Enable/disable User Service integration
    pub enabled: bool,
    /// Base URL for User Service API (e.g., http://localhost:4100/server/user)
    pub base_url: String,
    /// HTTP request timeout in seconds
    pub timeout_secs: u64,
    /// Number of retry attempts for failed requests
    pub retry_attempts: usize,
    /// OAuth token for inter-service authentication (from env: USER_SERVICE_AUTH_TOKEN)
    #[serde(skip)]
    pub auth_token: Option<String>,
}

impl Default for UserServiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://localhost:4100/server/user".to_string(),
            timeout_secs: 10,
            retry_attempts: 3,
            auth_token: None,
        }
    }
}

/// Payment Service connector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentServiceConfig {
    /// Enable/disable Payment Service integration
    pub enabled: bool,
    /// Base URL for Payment Service API (e.g., http://localhost:8000)
    pub base_url: String,
    /// HTTP request timeout in seconds
    pub timeout_secs: u64,
    /// Bearer token for authentication
    #[serde(skip)]
    pub auth_token: Option<String>,
}

impl Default for PaymentServiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://localhost:8000".to_string(),
            timeout_secs: 15,
            auth_token: None,
        }
    }
}

/// RabbitMQ Events configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventsConfig {
    /// Enable/disable async event publishing
    pub enabled: bool,
    /// AMQP connection string (amqp://user:password@host:port/%2f)
    pub amqp_url: String,
    /// Event exchange name
    pub exchange: String,
    /// Prefetch count for consumer
    pub prefetch: u16,
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            amqp_url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
            exchange: "stacker_events".to_string(),
            prefetch: 10,
        }
    }
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            user_service: Some(UserServiceConfig::default()),
            payment_service: Some(PaymentServiceConfig::default()),
            events: Some(EventsConfig::default()),
        }
    }
}
