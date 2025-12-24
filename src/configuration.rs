use serde;

#[derive(Debug, serde::Deserialize)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub app_port: u16,
    pub app_host: String,
    pub auth_url: String,
    pub max_clients_number: i64,
    pub amqp: AmqpSettings,
    pub vault: VaultSettings
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
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let address = std::env::var("VAULT_ADDRESS")
            .map_err(|_| config::ConfigError::NotFound("VAULT_ADDRESS".to_string()))?;
        let token = std::env::var("VAULT_TOKEN")
            .map_err(|_| config::ConfigError::NotFound("VAULT_TOKEN".to_string()))?;
        let agent_path_prefix = std::env::var("VAULT_AGENT_PATH_PREFIX")
            .unwrap_or_else(|_| "agent".to_string());
        
        Ok(VaultSettings {
            address,
            token,
            agent_path_prefix,
        })
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

    // Initialize our configuration reader
    let mut settings = config::Config::default();

    // Add configuration values from a file named `configuration`
    // with the .yaml extension
    settings.merge(config::File::with_name("configuration"))?; // .json, .toml, .yaml, .yml

    // Try to convert the configuration values it read into
    // our Settings type
    let mut config: Settings = settings.try_deserialize()?;
    
    // Load vault settings from environment variables
    config.vault = VaultSettings::from_env()?;
    
    Ok(config)
}
