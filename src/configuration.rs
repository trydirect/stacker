use serde;
use crate::connectors::ConnectorConfig;

#[derive(Debug, serde::Deserialize)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub app_port: u16,
    pub app_host: String,
    pub auth_url: String,
    pub max_clients_number: i64,
    pub amqp: AmqpSettings,
    pub vault: VaultSettings,
    #[serde(default)]
    pub connectors: ConnectorConfig,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            database: DatabaseSettings::default(),
            app_port: 8000,
            app_host: "127.0.0.1".to_string(),
            auth_url: "http://localhost:8080/me".to_string(),
            max_clients_number: 10,
            amqp: AmqpSettings::default(),
            vault: VaultSettings::default(),
            connectors: ConnectorConfig::default(),
        }
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

#[derive(Debug, serde::Deserialize, Clone)]
pub struct VaultSettings {
    pub address: String,
    pub token: String,
    pub agent_path_prefix: String,
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            address: "http://127.0.0.1:8200".to_string(),
            token: "dev-token".to_string(),
            agent_path_prefix: "agent".to_string(),
        }
    }
}

impl VaultSettings {
    /// Overlay Vault settings from environment variables, if present.
    /// If an env var is missing, keep the existing file-provided value.
    pub fn overlay_env(self) -> Self {
        let address = std::env::var("VAULT_ADDRESS").unwrap_or(self.address);
        let token = std::env::var("VAULT_TOKEN").unwrap_or(self.token);
        let agent_path_prefix =
            std::env::var("VAULT_AGENT_PATH_PREFIX").unwrap_or(self.agent_path_prefix);

        VaultSettings {
            address,
            token,
            agent_path_prefix,
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

    Ok(config)
}
