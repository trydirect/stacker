use serde;

#[derive(Debug, serde::Deserialize)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub app_port: u16,
    pub app_host: String,
    pub auth_url: String,
    pub max_clients_number: i64,
    pub amqp: AmqpSettings,
    pub vault: VaultSettings,
}

#[derive(Debug, serde::Deserialize)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub database_name: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct AmqpSettings {
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, serde::Deserialize)]
pub struct VaultSettings {
    pub address: String,
    pub token: String,
    pub agent_path_prefix: String,
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

    // Prefer real config, fall back to dist sample, require at least one to exist
    let settings = config::Config::builder()
        .add_source(
            config::File::with_name("configuration.yaml")
                .required(false)
        )
        .add_source(
            config::File::with_name("configuration.yaml.dist")
                .required(false)
        )
        .build()?;

    // Try to convert the configuration values it read into our Settings type
    let mut config: Settings = settings.try_deserialize()?;

    // Overlay Vault settings with environment variables if present
    config.vault = config.vault.overlay_env();

    Ok(config)
}
