use crate::connectors::ConnectorConfig;
use serde;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub app_port: u16,
    pub app_host: String,
    pub auth_url: String,
    #[serde(default = "Settings::default_user_service_url")]
    pub user_service_url: String,
    pub max_clients_number: i64,
    #[serde(default = "Settings::default_agent_command_poll_timeout_secs")]
    pub agent_command_poll_timeout_secs: u64,
    #[serde(default = "Settings::default_agent_command_poll_interval_secs")]
    pub agent_command_poll_interval_secs: u64,
    #[serde(default = "Settings::default_casbin_reload_enabled")]
    pub casbin_reload_enabled: bool,
    #[serde(default = "Settings::default_casbin_reload_interval_secs")]
    pub casbin_reload_interval_secs: u64,
    #[serde(default)]
    pub amqp: AmqpSettings,
    #[serde(default)]
    pub vault: VaultSettings,
    #[serde(default)]
    pub connectors: ConnectorConfig,
    #[serde(default)]
    pub deployment: DeploymentSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            database: DatabaseSettings::default(),
            app_port: 8000,
            app_host: "127.0.0.1".to_string(),
            auth_url: "http://localhost:8080/me".to_string(),
            user_service_url: Self::default_user_service_url(),
            max_clients_number: 10,
            agent_command_poll_timeout_secs: Self::default_agent_command_poll_timeout_secs(),
            agent_command_poll_interval_secs: Self::default_agent_command_poll_interval_secs(),
            casbin_reload_enabled: Self::default_casbin_reload_enabled(),
            casbin_reload_interval_secs: Self::default_casbin_reload_interval_secs(),
            amqp: AmqpSettings::default(),
            vault: VaultSettings::default(),
            connectors: ConnectorConfig::default(),
            deployment: DeploymentSettings::default(),
        }
    }
}

impl Settings {
    fn default_user_service_url() -> String {
        "http://user:4100".to_string()
    }

    fn default_agent_command_poll_timeout_secs() -> u64 {
        30
    }

    fn default_agent_command_poll_interval_secs() -> u64 {
        3
    }

    fn default_casbin_reload_enabled() -> bool {
        true
    }

    fn default_casbin_reload_interval_secs() -> u64 {
        10
    }
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub database_name: String,
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            username: "postgres".to_string(),
            password: "postgres".to_string(),
            host: "127.0.0.1".to_string(),
            port: 5432,
            database_name: "stacker".to_string(),
        }
    }
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct AmqpSettings {
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: u16,
}

impl Default for AmqpSettings {
    fn default() -> Self {
        Self {
            username: "guest".to_string(),
            password: "guest".to_string(),
            host: "127.0.0.1".to_string(),
            port: 5672,
        }
    }
}

/// Deployment-related settings for app configuration paths
#[derive(Debug, serde::Deserialize, Clone)]
pub struct DeploymentSettings {
    /// Base path for app config files on the deployment server
    /// Default: /home/trydirect
    #[serde(default = "DeploymentSettings::default_config_base_path")]
    pub config_base_path: String,
}

impl Default for DeploymentSettings {
    fn default() -> Self {
        Self {
            config_base_path: Self::default_config_base_path(),
        }
    }
}

impl DeploymentSettings {
    fn default_config_base_path() -> String {
        "/home/trydirect".to_string()
    }
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct VaultSettings {
    pub address: String,
    pub token: String,
    pub agent_path_prefix: String,
    #[serde(default = "VaultSettings::default_api_prefix")]
    pub api_prefix: String,
    #[serde(default)]
    pub ssh_key_path_prefix: Option<String>,
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            address: "http://127.0.0.1:8200".to_string(),
            token: "dev-token".to_string(),
            agent_path_prefix: "agent".to_string(),
            api_prefix: Self::default_api_prefix(),
            ssh_key_path_prefix: Some("users".to_string()),
        }
    }
}

impl VaultSettings {
    fn default_api_prefix() -> String {
        "v1".to_string()
    }

    /// Overlay Vault settings from environment variables, if present.
    /// If an env var is missing, keep the existing file-provided value.
    pub fn overlay_env(self) -> Self {
        let address = std::env::var("VAULT_ADDRESS").unwrap_or(self.address);
        let token = std::env::var("VAULT_TOKEN").unwrap_or(self.token);
        let agent_path_prefix =
            std::env::var("VAULT_AGENT_PATH_PREFIX").unwrap_or(self.agent_path_prefix);
        let api_prefix = std::env::var("VAULT_API_PREFIX").unwrap_or(self.api_prefix);
        let ssh_key_path_prefix = std::env::var("VAULT_SSH_KEY_PATH_PREFIX").unwrap_or(
            self.ssh_key_path_prefix
                .unwrap_or_else(|| "users".to_string()),
        );

        VaultSettings {
            address,
            token,
            agent_path_prefix,
            api_prefix,
            ssh_key_path_prefix: Some(ssh_key_path_prefix),
        }
    }
}

impl DatabaseSettings {
    // Connection string: postgresql://<username>:<password>@<host>:<port>/<database_name>
    pub fn connection_string(&self) -> String {
        format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database_name,
        )
    }

    pub fn connection_string_without_db(&self) -> String {
        format!(
            "postgresql://{}:{}@{}:{}",
            self.username, self.password, self.host, self.port,
        )
    }
}

impl AmqpSettings {
    pub fn connection_string(&self) -> String {
        format!(
            "amqp://{}:{}@{}:{}/%2f",
            self.username, self.password, self.host, self.port,
        )
    }
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Start with defaults
    let mut config = Settings::default();

    // Prefer real config, fall back to dist samples; layer multiple formats
    let settings = config::Config::builder()
        // Primary local config
        .add_source(config::File::with_name("configuration.yaml").required(false))
        .add_source(config::File::with_name("configuration.yml").required(false))
        .add_source(config::File::with_name("configuration").required(false))
        // Fallback samples
        .add_source(config::File::with_name("configuration.yaml.dist").required(false))
        .add_source(config::File::with_name("configuration.yml.dist").required(false))
        .add_source(config::File::with_name("configuration.dist").required(false))
        .build()?;

    // Try to convert the configuration values it read into our Settings type
    if let Ok(loaded) = settings.try_deserialize::<Settings>() {
        config = loaded;
    }

    // Overlay Vault settings with environment variables if present
    config.vault = config.vault.overlay_env();

    if let Ok(timeout) = std::env::var("STACKER_AGENT_POLL_TIMEOUT_SECS") {
        if let Ok(parsed) = timeout.parse::<u64>() {
            config.agent_command_poll_timeout_secs = parsed;
        }
    }

    if let Ok(interval) = std::env::var("STACKER_AGENT_POLL_INTERVAL_SECS") {
        if let Ok(parsed) = interval.parse::<u64>() {
            config.agent_command_poll_interval_secs = parsed;
        }
    }

    if let Ok(enabled) = std::env::var("STACKER_CASBIN_RELOAD_ENABLED") {
        config.casbin_reload_enabled = matches!(enabled.as_str(), "1" | "true" | "TRUE");
    }

    if let Ok(interval) = std::env::var("STACKER_CASBIN_RELOAD_INTERVAL_SECS") {
        if let Ok(parsed) = interval.parse::<u64>() {
            config.casbin_reload_interval_secs = parsed;
        }
    }

    // Overlay AMQP settings with environment variables if present
    if let Ok(host) = std::env::var("AMQP_HOST") {
        config.amqp.host = host;
    }
    if let Ok(port) = std::env::var("AMQP_PORT") {
        if let Ok(parsed) = port.parse::<u16>() {
            config.amqp.port = parsed;
        }
    }
    if let Ok(username) = std::env::var("AMQP_USERNAME") {
        config.amqp.username = username;
    }
    if let Ok(password) = std::env::var("AMQP_PASSWORD") {
        config.amqp.password = password;
    }

    // Overlay Deployment settings with environment variables if present
    if let Ok(base_path) = std::env::var("DEPLOYMENT_CONFIG_BASE_PATH") {
        config.deployment.config_base_path = base_path;
    }

    Ok(config)
}
