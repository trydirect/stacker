use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
use serde_valid::Validate;

use crate::cli::error::{CliError, Severity, ValidationIssue};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// AppType — discoverable project types
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppType {
    Static,
    Node,
    Python,
    Rust,
    Go,
    Php,
    Custom,
}

impl fmt::Display for AppType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static => write!(f, "static"),
            Self::Node => write!(f, "node"),
            Self::Python => write!(f, "python"),
            Self::Rust => write!(f, "rust"),
            Self::Go => write!(f, "go"),
            Self::Php => write!(f, "php"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

impl Default for AppType {
    fn default() -> Self {
        Self::Static
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DeployTarget — where to deploy
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeployTarget {
    Local,
    Cloud,
    Server,
}

impl fmt::Display for DeployTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Cloud => write!(f, "cloud"),
            Self::Server => write!(f, "server"),
        }
    }
}

impl Default for DeployTarget {
    fn default() -> Self {
        Self::Local
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ProxyType — reverse proxy flavors
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProxyType {
    Nginx,
    NginxProxyManager,
    Traefik,
    Caddy,
    None,
}

impl fmt::Display for ProxyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nginx => write!(f, "nginx"),
            Self::NginxProxyManager => write!(f, "nginx-proxy-manager"),
            Self::Traefik => write!(f, "traefik"),
            Self::Caddy => write!(f, "caddy"),
            Self::None => write!(f, "none"),
        }
    }
}

impl Default for ProxyType {
    fn default() -> Self {
        Self::None
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SslMode — certificate handling
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SslMode {
    Auto,
    Manual,
    Off,
}

impl Default for SslMode {
    fn default() -> Self {
        Self::Off
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// AiProviderType — supported LLM providers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderType {
    Openai,
    Anthropic,
    Ollama,
    Custom,
}

impl fmt::Display for AiProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Openai => write!(f, "openai"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::Ollama => write!(f, "ollama"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

impl Default for AiProviderType {
    fn default() -> Self {
        Self::Openai
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CloudProvider — supported cloud infrastructure providers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloudProvider {
    Hetzner,
    Digitalocean,
    Aws,
    Linode,
    Vultr,
    Contabo,
}

/// Cloud orchestration mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CloudOrchestrator {
    Local,
    #[default]
    Remote,
}

impl fmt::Display for CloudProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hetzner => write!(f, "hetzner"),
            Self::Digitalocean => write!(f, "digitalocean"),
            Self::Aws => write!(f, "aws"),
            Self::Linode => write!(f, "linode"),
            Self::Vultr => write!(f, "vultr"),
            Self::Contabo => write!(f, "contabo"),
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Configuration structs — nested sections of stacker.yml
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Application source configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSource {
    #[serde(rename = "type", default)]
    pub app_type: AppType,

    #[serde(default = "default_app_path")]
    pub path: PathBuf,

    #[serde(default)]
    pub dockerfile: Option<PathBuf>,

    #[serde(default)]
    pub image: Option<String>,

    #[serde(default)]
    pub build: Option<BuildConfig>,

    /// Explicit port mappings (e.g. `"8080:80"`).  When empty the CLI
    /// derives a default from `app_type`.
    #[serde(default)]
    pub ports: Vec<String>,

    /// Volume mounts (e.g. `"./data:/app/data"`).
    #[serde(default)]
    pub volumes: Vec<String>,

    /// Per-app environment variables.  Merged with the top-level `env:`
    /// section (app-level wins on conflict).
    #[serde(default)]
    pub environment: HashMap<String, String>,

    /// Override the container CMD.  Maps to `command:` in docker-compose.
    #[serde(default)]
    pub command: Option<String>,

    /// Docker compose healthcheck for this service.
    #[serde(default)]
    pub healthcheck: Option<ComposeHealthcheck>,
}

fn default_app_path() -> PathBuf {
    PathBuf::from(".")
}

/// Docker build configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildConfig {
    #[serde(default = "default_build_context")]
    pub context: String,

    #[serde(default)]
    pub args: HashMap<String, String>,
}

fn default_build_context() -> String {
    ".".to_string()
}

fn default_health_timeout_compose() -> String {
    "30s".to_string()
}

fn default_health_retries_compose() -> u32 {
    3
}

/// Docker compose healthcheck definition for a service.
///
/// This is distinct from `MonitoringConfig::healthcheck`, which is an
/// app-level HTTP endpoint polling configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComposeHealthcheck {
    pub test: String,
    #[serde(default = "default_health_interval")]
    pub interval: String,
    #[serde(default = "default_health_timeout_compose")]
    pub timeout: String,
    #[serde(default = "default_health_retries_compose")]
    pub retries: u32,
}

/// Additional container service alongside the app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    pub name: String,
    pub image: String,

    #[serde(default)]
    pub ports: Vec<String>,

    #[serde(default)]
    pub environment: HashMap<String, String>,

    #[serde(default)]
    pub volumes: Vec<String>,

    #[serde(default)]
    pub depends_on: Vec<String>,

    /// Override the container CMD.  Maps to `command:` in docker-compose.
    #[serde(default)]
    pub command: Option<String>,

    /// Docker compose healthcheck for this service.
    #[serde(default)]
    pub healthcheck: Option<ComposeHealthcheck>,
}

fn deserialize_services<'de, D>(deserializer: D) -> Result<Vec<ServiceDefinition>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;

    match value {
        serde_yaml::Value::Null => Ok(Vec::new()),
        serde_yaml::Value::Sequence(_) => {
            serde_yaml::from_value(value).map_err(serde::de::Error::custom)
        }
        serde_yaml::Value::Mapping(map) => {
            let mut services = Vec::new();

            for (key, service_value) in map {
                let service_key = key
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("services map key must be a string"))?
                    .to_string();

                let mut service_map = match service_value {
                    serde_yaml::Value::Mapping(m) => m,
                    _ => {
                        return Err(serde::de::Error::custom(
                            "each services map item must be an object",
                        ));
                    }
                };

                let has_name = service_map.keys().any(|k| k.as_str() == Some("name"));
                if !has_name {
                    service_map.insert(
                        serde_yaml::Value::String("name".to_string()),
                        serde_yaml::Value::String(service_key),
                    );
                }

                let service: ServiceDefinition =
                    serde_yaml::from_value(serde_yaml::Value::Mapping(service_map))
                        .map_err(serde::de::Error::custom)?;
                services.push(service);
            }

            Ok(services)
        }
        _ => Err(serde::de::Error::custom(
            "services must be a sequence or map",
        )),
    }
}

/// Proxy/ingress configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyConfig {
    #[serde(rename = "type", default)]
    pub proxy_type: ProxyType,

    #[serde(default = "default_auto_detect")]
    pub auto_detect: bool,

    #[serde(default)]
    pub domains: Vec<DomainConfig>,

    #[serde(default)]
    pub config: Option<PathBuf>,
}

fn default_auto_detect() -> bool {
    true
}

/// Per-domain routing and SSL settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainConfig {
    pub domain: String,

    #[serde(default)]
    pub ssl: SslMode,

    pub upstream: String,
}

/// A declaratively-defined pipe (`pipes:` block in stacker.yml). This is the
/// committable source of truth reconciled by `stacker pipe apply` / `pipe diff`
/// against the deployed templates+instances. Fields mirror the `pipe create`
/// flags so the imperative and declarative surfaces stay 1:1.
///
/// See `config/docs/PIPE_IAC_AND_RESILIENCE_PLAN.md` §5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipeSpec {
    /// Unique pipe name (identity for reconcile).
    pub name: String,
    /// Source app code (container/service selector).
    pub source: String,
    /// Target app code.
    pub target: String,
    /// Source endpoint "METHOD /path".
    pub source_endpoint: String,
    /// Target endpoint "METHOD /path".
    pub target_endpoint: String,
    #[serde(default)]
    pub source_fields: Vec<String>,
    #[serde(default)]
    pub target_fields: Vec<String>,
    /// Trigger mode: manual | webhook | poll (default webhook, matching the CLI).
    #[serde(default = "default_pipe_trigger")]
    pub trigger: String,
    /// Poll interval (seconds) when trigger = poll.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval: Option<u32>,
    /// Max delivery retries (→ pipe config).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_backoff_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_backoff_max_ms: Option<u64>,
    /// Run another pipe (by name) on failure / success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_success: Option<String>,
}

fn default_pipe_trigger() -> String {
    "webhook".to_string()
}

impl PipeSpec {
    /// Build the typed resilience/lifecycle config for this pipe (empty when no
    /// retry/handler was declared, so it round-trips cleanly).
    pub fn to_pipe_config(&self) -> crate::models::pipe_config::PipeConfig {
        use crate::models::agent_protocol::RetryPolicy;
        use crate::models::pipe_config::{HandlerRef, PipeConfig};

        let d = RetryPolicy::default();
        let retry = if self.retry.is_some()
            || self.retry_backoff_ms.is_some()
            || self.retry_backoff_max_ms.is_some()
        {
            Some(RetryPolicy {
                max_retries: self.retry.unwrap_or(d.max_retries),
                backoff_base_ms: self.retry_backoff_ms.unwrap_or(d.backoff_base_ms),
                backoff_max_ms: self.retry_backoff_max_ms.unwrap_or(d.backoff_max_ms),
            })
        } else {
            None
        };
        PipeConfig {
            retry,
            on_failure: self.on_failure.clone().map(HandlerRef::Pipe),
            on_success: self.on_success.clone().map(HandlerRef::Pipe),
        }
    }
}

/// Docker registry credentials for pulling private images during deployment.
///
/// TODO: Currently these credentials are passed through on every deploy (env vars or stacker.yml).
/// In the future, store docker credentials server-side (similar to how `cloud_token` is persisted
/// in the `clouds` table) or in HashiCorp Vault, so users only need to provide them once.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryConfig {
    /// Docker registry username (or from env `STACKER_DOCKER_USERNAME`).
    #[serde(default)]
    pub username: Option<String>,

    /// Docker registry password (or from env `STACKER_DOCKER_PASSWORD`).
    #[serde(default)]
    pub password: Option<String>,

    /// Docker registry server URL (default: docker.io).
    /// Use for private registries like `ghcr.io`, `registry.example.com`.
    #[serde(default)]
    pub server: Option<String>,
}

/// Per-target deployment profile in multi-target configs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeployProfileConfig {
    #[serde(default)]
    pub environment: Option<String>,

    #[serde(default)]
    pub compose_file: Option<PathBuf>,

    #[serde(default)]
    pub deployment_hash: Option<String>,

    #[serde(default)]
    pub cloud: Option<CloudConfig>,

    #[serde(default)]
    pub server: Option<ServerConfig>,

    #[serde(default)]
    pub registry: Option<RegistryConfig>,
}

impl DeployProfileConfig {
    fn inferred_target(&self, profile_name: &str) -> Result<DeployTarget, CliError> {
        match (self.server.is_some(), self.cloud.is_some()) {
            (true, true) => Err(CliError::ConfigValidation(format!(
                "deploy.targets.{profile_name} cannot define both 'server' and 'cloud'"
            ))),
            (true, false) => Ok(DeployTarget::Server),
            (false, true) => Ok(DeployTarget::Cloud),
            (false, false) => Ok(DeployTarget::Local),
        }
    }
}

/// Deployment target configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeployConfig {
    #[serde(default)]
    pub target: DeployTarget,

    #[serde(default)]
    pub environment: Option<String>,

    #[serde(default)]
    pub compose_file: Option<PathBuf>,

    #[serde(default)]
    pub deployment_hash: Option<String>,

    #[serde(default)]
    pub cloud: Option<CloudConfig>,

    #[serde(default)]
    pub server: Option<ServerConfig>,

    /// Docker registry credentials for pulling private images.
    #[serde(default)]
    pub registry: Option<RegistryConfig>,

    /// Default named target when `deploy.targets` is used.
    #[serde(default)]
    pub default_target: Option<String>,

    /// Named deploy profiles. When present, commands resolve one target profile
    /// to the legacy single-target shape before executing.
    #[serde(default)]
    pub targets: BTreeMap<String, DeployProfileConfig>,
}

impl DeployConfig {
    pub fn uses_named_targets(&self) -> bool {
        !self.targets.is_empty()
    }

    fn parse_legacy_target_override(value: &str) -> Result<DeployTarget, CliError> {
        let json = format!("\"{}\"", value.trim().to_lowercase());
        serde_json::from_str::<DeployTarget>(&json).map_err(|_| {
            CliError::ConfigValidation(format!(
                "Unknown deploy target '{}'. Valid targets: local, cloud, server",
                value
            ))
        })
    }

    fn resolve_named_target_name(&self, requested: Option<&str>) -> Result<String, CliError> {
        if let Some(requested_name) = requested.map(str::trim).filter(|value| !value.is_empty()) {
            if self.targets.contains_key(requested_name) {
                return Ok(requested_name.to_string());
            }

            return Err(CliError::ConfigValidation(format!(
                "Unknown deploy target profile '{}'. Available targets: {}",
                requested_name,
                self.targets.keys().cloned().collect::<Vec<_>>().join(", ")
            )));
        }

        if let Some(default_target) = self
            .default_target
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if self.targets.contains_key(default_target) {
                return Ok(default_target.to_string());
            }

            return Err(CliError::ConfigValidation(format!(
                "deploy.default_target '{}' does not match any entry in deploy.targets",
                default_target
            )));
        }

        if self.targets.len() == 1 {
            return Ok(self
                .targets
                .keys()
                .next()
                .expect("single target must have a name")
                .clone());
        }

        Err(CliError::ConfigValidation(
            "deploy.default_target is required when deploy.targets defines multiple entries"
                .to_string(),
        ))
    }

    pub fn resolve(&self, requested: Option<&str>) -> Result<DeployConfig, CliError> {
        if !self.uses_named_targets() {
            let mut resolved = self.clone();
            if let Some(target_name) = requested.map(str::trim).filter(|value| !value.is_empty()) {
                resolved.target = Self::parse_legacy_target_override(target_name)?;
            }
            return Ok(resolved);
        }

        let profile_name = self.resolve_named_target_name(requested)?;
        let profile = self.targets.get(&profile_name).expect("target exists");
        let inferred_target = profile.inferred_target(&profile_name)?;

        Ok(DeployConfig {
            target: inferred_target,
            environment: profile
                .environment
                .clone()
                .or_else(|| self.environment.clone()),
            compose_file: profile.compose_file.clone(),
            deployment_hash: profile.deployment_hash.clone(),
            cloud: profile.cloud.clone(),
            server: profile.server.clone(),
            registry: profile.registry.clone(),
            default_target: self.default_target.clone(),
            targets: self.targets.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentConfig {
    #[serde(default)]
    pub compose_file: Option<PathBuf>,

    #[serde(default)]
    pub env_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstallConfig {
    #[serde(default)]
    pub inputs: serde_json::Map<String, serde_json::Value>,
}

/// Cloud provider settings for cloud deployments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    pub provider: CloudProvider,

    #[serde(default)]
    pub orchestrator: CloudOrchestrator,

    #[serde(default)]
    pub region: Option<String>,

    #[serde(default)]
    pub size: Option<String>,

    #[serde(default)]
    pub install_image: Option<String>,

    #[serde(default)]
    pub remote_payload_file: Option<PathBuf>,

    #[serde(default)]
    pub ssh_key: Option<PathBuf>,

    /// Name of saved cloud credential on the Stacker server.
    /// Used with `stacker deploy --key devops` or `deploy.cloud.key: devops` in stacker.yml.
    /// When set, the CLI looks up saved credentials by provider instead of requiring env vars.
    #[serde(default)]
    pub key: Option<String>,

    /// Name of a saved server on the Stacker server.
    /// Used with `stacker deploy --server bastion` or `deploy.cloud.server: bastion` in stacker.yml.
    /// When set, the CLI passes the server_id to the deploy form so it is reused.
    #[serde(default)]
    pub server: Option<String>,

    /// Public ports to open in the cloud provider firewall after deployment.
    /// Each entry is a port number or "port/protocol" string (e.g. "8000" or "8000/tcp").
    /// These are sent to the Install Service to configure provider-level firewall rules.
    #[serde(default)]
    pub public_ports: Vec<String>,
}

/// Remote server settings for server deployments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,

    #[serde(default = "default_ssh_user")]
    pub user: String,

    #[serde(default)]
    pub ssh_key: Option<PathBuf>,

    #[serde(default = "default_ssh_port")]
    pub port: u16,
}

fn default_ssh_user() -> String {
    "root".to_string()
}

fn default_ssh_port() -> u16 {
    22
}

impl Default for ServerConfig {
    /// An empty-host skeleton with the same field defaults serde applies
    /// (`user = root`, `port = 22`). Used when building a server config from
    /// CLI flags (`--server-host/--server-user/--server-ssh-key`).
    fn default() -> Self {
        Self {
            host: String::new(),
            user: default_ssh_user(),
            ssh_key: None,
            port: default_ssh_port(),
        }
    }
}

/// Default AI request timeout in seconds.
fn default_ai_timeout() -> u64 {
    300
}

/// AI/LLM assistant configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub provider: AiProviderType,

    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub api_key: Option<String>,

    #[serde(default)]
    pub endpoint: Option<String>,

    /// Request timeout in seconds. Default: 300 (5 minutes).
    /// Can be overridden via `STACKER_AI_TIMEOUT` env var.
    #[serde(default = "default_ai_timeout")]
    pub timeout: u64,

    #[serde(default)]
    pub tasks: Vec<String>,
}

/// Monitoring and health check configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonitoringConfig {
    #[serde(default)]
    pub status_panel: bool,

    #[serde(default)]
    pub healthcheck: Option<HealthcheckConfig>,

    #[serde(default)]
    pub metrics: Option<MetricsConfig>,

    /// Container-down alerting for `stacker monitor`. Absent → no alarm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alerts: Option<AlertConfig>,
}

/// Config for the `stacker monitor` container-health alarm (§10 of the PIPE
/// IaC/resilience plan). Reuses `HandlerRef` as the notification target, so an
/// alert can hit a webhook (ntfy/Slack) or, later, run a pipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Poll interval in seconds for the watch loop (default 60).
    #[serde(default = "default_alert_interval")]
    pub interval: u64,
    /// Where to deliver the alert (required).
    pub target: AlertTarget,
    /// Also notify when containers recover to healthy (default true).
    #[serde(default = "default_true")]
    pub on_recovery: bool,
}

/// Alert delivery target — a YAML-friendly, untagged view (distinguished by the
/// `url` vs `pipe` key) that maps onto the shared `HandlerRef` for dispatch:
///
/// ```yaml
/// target: { terminal: true }                                  # terminal + desktop
/// target: { url: "https://ntfy.example.com/alerts", method: POST }   # webhook
/// target: { pipe: oncall-notify }                             # run a pipe
/// ```
///
/// (A dedicated type, rather than reusing `HandlerRef` directly, because
/// `serde_yaml` renders externally-tagged enums as `!tag` — awkward in a config
/// file — whereas this untagged form reads naturally.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AlertTarget {
    /// Terminal + desktop notification (OS notification, terminal bell, stderr).
    Terminal { terminal: bool },
    /// HTTP webhook (ntfy/Slack/…). `method` defaults to POST.
    Webhook {
        url: String,
        #[serde(default = "default_notify_method_alert")]
        method: String,
    },
    /// Run a declared pipe by name.
    Pipe { pipe: String },
}

fn default_notify_method_alert() -> String {
    "POST".to_string()
}

fn default_alert_interval() -> u64 {
    60
}

fn default_true() -> bool {
    true
}

/// Healthcheck settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthcheckConfig {
    #[serde(default = "default_health_endpoint")]
    pub endpoint: String,

    #[serde(default = "default_health_interval")]
    pub interval: String,
}

fn default_health_endpoint() -> String {
    "/health".to_string()
}

fn default_health_interval() -> String {
    "30s".to_string()
}

/// Metrics collection settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub telegraf: bool,
}

/// Lifecycle hook commands.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookConfig {
    #[serde(default)]
    pub pre_build: Option<PathBuf>,

    #[serde(default)]
    pub post_deploy: Option<PathBuf>,

    #[serde(default)]
    pub on_failure: Option<PathBuf>,

    /// Send a terminal/desktop notification on deploy completion.
    #[serde(default, skip_serializing_if = "is_false")]
    pub notify: bool,
}

/// Serde helper: skip serializing when `false`.
fn is_false(b: &bool) -> bool {
    !*b
}

/// Project identity metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    /// Registered User Service identity used as remote deploy payload `stack_code`.
    #[serde(default)]
    pub identity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigContract {
    #[serde(default)]
    pub services: BTreeMap<String, TargetConfigContract>,
}

/// Who controls a declared field's final value at install time.
///
/// Mirrors `shared-fixtures/api-contracts/marketplace-field-policy.json`
/// (`stacker/tests/contracts/marketplace-field-policy.contract.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mutability {
    /// Author's value is baked in; never exposed to an installer.
    Fixed,
    /// Author's value is only a default; an installer's override can replace it.
    Editable,
    /// The system produces a fresh value per install; the installer never enters it.
    Generated,
}

/// Generator/validation shape for a field's value, selected via `type:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Hex,
    Base64,
    Alphanumeric,
    Uuid,
    Enum,
    DerivedJwt,
}

/// Declared policy for one `config_contract.services.<service>.fields.<NAME>` entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldPolicy {
    pub mutability: Mutability,
    pub required: bool,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_spec: Option<FieldType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alg: Option<String>,
}

fn default_field_required() -> bool {
    true
}

/// Wire shape used to deserialize a `FieldPolicy`, before the conditional
/// validation below (generated needs a type; derived_jwt needs its signing
/// trio; enum needs a value set) — matches the shared contract's `allOf`/`if`
/// rules, since plain serde derives can't express those.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct RawFieldPolicy {
    mutability: Mutability,
    #[serde(default = "default_field_required")]
    required: bool,
    #[serde(rename = "type", default)]
    type_spec: Option<FieldType>,
    #[serde(default)]
    length: Option<usize>,
    #[serde(default)]
    min_length: Option<usize>,
    #[serde(default)]
    values: Vec<String>,
    #[serde(default)]
    signing_key: Option<String>,
    #[serde(default)]
    claims: Option<serde_json::Value>,
    #[serde(default)]
    alg: Option<String>,
}

impl<'de> Deserialize<'de> for FieldPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawFieldPolicy::deserialize(deserializer)?;

        if raw.mutability == Mutability::Generated && raw.type_spec.is_none() {
            return Err(serde::de::Error::custom(
                "field policy with mutability: generated must declare a type",
            ));
        }

        if raw.type_spec == Some(FieldType::DerivedJwt)
            && (raw.signing_key.is_none() || raw.claims.is_none() || raw.alg.is_none())
        {
            return Err(serde::de::Error::custom(
                "field policy with type: derived_jwt must declare signing_key, claims, and alg",
            ));
        }

        if raw.type_spec == Some(FieldType::Enum) && raw.values.is_empty() {
            return Err(serde::de::Error::custom(
                "field policy with type: enum must declare a non-empty values list",
            ));
        }

        Ok(FieldPolicy {
            mutability: raw.mutability,
            required: raw.required,
            type_spec: raw.type_spec,
            length: raw.length,
            min_length: raw.min_length,
            values: raw.values,
            signing_key: raw.signing_key,
            claims: raw.claims,
            alg: raw.alg,
        })
    }
}

impl FieldPolicy {
    /// A `mutability: fixed` policy with no generator/validation constraints —
    /// the shape a legacy `required:`/`optional:` list entry maps to.
    pub fn fixed(required: bool) -> Self {
        FieldPolicy {
            mutability: Mutability::Fixed,
            required,
            type_spec: None,
            length: None,
            min_length: None,
            values: Vec::new(),
            signing_key: None,
            claims: None,
            alg: None,
        }
    }

    /// The default generator a legacy `secret:` list entry maps to: a
    /// 32-char alphanumeric value, matching `generate_secret()`'s convention
    /// (`console/commands/cli/service.rs`).
    pub fn default_generated_secret() -> Self {
        FieldPolicy {
            mutability: Mutability::Generated,
            required: true,
            type_spec: Some(FieldType::Alphanumeric),
            length: None,
            min_length: Some(32),
            values: Vec::new(),
            signing_key: None,
            claims: None,
            alg: None,
        }
    }

    fn is_plain_fixed(&self) -> bool {
        self.mutability == Mutability::Fixed
            && self.type_spec.is_none()
            && self.length.is_none()
            && self.min_length.is_none()
            && self.values.is_empty()
            && self.signing_key.is_none()
            && self.claims.is_none()
            && self.alg.is_none()
    }

    fn is_default_generated_secret(&self) -> bool {
        self.mutability == Mutability::Generated
            && self.required
            && self.type_spec == Some(FieldType::Alphanumeric)
            && self.length.is_none()
            && self.min_length == Some(32)
            && self.values.is_empty()
            && self.signing_key.is_none()
            && self.claims.is_none()
            && self.alg.is_none()
    }
}

/// Per-service field policy declarations.
///
/// Backed by a single `fields: HashMap<String, FieldPolicy>`. Accepts and
/// still emits the legacy `required`/`optional`/`secret` plain-string-list
/// shape (mapped to `fixed`/`fixed`/`generated` policies respectively) so
/// existing `stacker.yml` files keep parsing unchanged; new contracts can use
/// the richer `fields:` map directly.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TargetConfigContract {
    pub fields: HashMap<String, FieldPolicy>,
}

impl TargetConfigContract {
    /// Build a contract from the legacy three-list shape, used by
    /// `stacker config suggest-contract` and any other code still producing
    /// the plain required/optional/secret form.
    pub fn from_legacy_lists(
        required: Vec<String>,
        optional: Vec<String>,
        secret: Vec<String>,
    ) -> Self {
        let mut fields = HashMap::new();
        // `secret` wins when a key appears in both lists (a key can be both
        // required and secret in the legacy shape) — process it first with
        // an overwriting insert so a later required/optional entry for the
        // same key can't demote it back to `fixed`.
        for key in secret {
            fields.insert(key, FieldPolicy::default_generated_secret());
        }
        for key in required {
            fields
                .entry(key)
                .or_insert_with(|| FieldPolicy::fixed(true));
        }
        for key in optional {
            fields
                .entry(key)
                .or_insert_with(|| FieldPolicy::fixed(false));
        }
        TargetConfigContract { fields }
    }

    fn keys_where(&self, predicate: impl Fn(&FieldPolicy) -> bool) -> Vec<String> {
        let mut keys: Vec<String> = self
            .fields
            .iter()
            .filter(|(_, policy)| predicate(policy))
            .map(|(name, _)| name.clone())
            .collect();
        keys.sort();
        keys
    }

    /// Fields declared `required: true`, regardless of mutability — a
    /// `generated` field can also be `required` (it must resolve to a value
    /// even though the installer never types it in). Used by `stacker config
    /// check`'s "must be present locally" semantics, which is orthogonal to
    /// who controls the value.
    pub fn required_keys(&self) -> Vec<String> {
        self.keys_where(|p| p.required)
    }

    /// Fields declared `required: false`, regardless of mutability.
    pub fn optional_keys(&self) -> Vec<String> {
        self.keys_where(|p| !p.required)
    }

    /// Fields declared `mutability: generated` — the ones a marketplace
    /// install must produce a fresh value for.
    pub fn secret_keys(&self) -> Vec<String> {
        self.keys_where(|p| p.mutability == Mutability::Generated)
    }

    /// Fields declared `mutability: editable` — the ones an installer's
    /// form/env can override, with the author's value as the default.
    pub fn editable_keys(&self) -> Vec<String> {
        self.keys_where(|p| p.mutability == Mutability::Editable)
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawTargetConfigContract {
    required: Vec<String>,
    optional: Vec<String>,
    secret: Vec<String>,
    fields: HashMap<String, FieldPolicy>,
}

impl<'de> Deserialize<'de> for TargetConfigContract {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawTargetConfigContract::deserialize(deserializer)?;
        // Explicit `fields:` entries take precedence; among the legacy lists,
        // `secret` wins when a key appears in both `secret` and
        // `required`/`optional` (a key can be both required and secret).
        let mut fields = raw.fields;

        for key in raw.secret {
            fields
                .entry(key)
                .or_insert_with(FieldPolicy::default_generated_secret);
        }
        for key in raw.required {
            fields
                .entry(key)
                .or_insert_with(|| FieldPolicy::fixed(true));
        }
        for key in raw.optional {
            fields
                .entry(key)
                .or_insert_with(|| FieldPolicy::fixed(false));
        }

        Ok(TargetConfigContract { fields })
    }
}

#[derive(Serialize)]
struct SerializedTargetConfigContract {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    required: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    optional: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    secret: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    fields: BTreeMap<String, FieldPolicy>,
}

impl Serialize for TargetConfigContract {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut required = Vec::new();
        let mut optional = Vec::new();
        let mut secret = Vec::new();
        let mut fields = BTreeMap::new();

        let mut sorted: Vec<_> = self.fields.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));

        for (name, policy) in sorted {
            if policy.is_plain_fixed() {
                if policy.required {
                    required.push(name.clone());
                } else {
                    optional.push(name.clone());
                }
            } else if policy.is_default_generated_secret() {
                secret.push(name.clone());
            } else {
                fields.insert(name.clone(), policy.clone());
            }
        }

        SerializedTargetConfigContract {
            required,
            optional,
            secret,
            fields,
        }
        .serialize(serializer)
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// StackerConfig — the root configuration type
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Marker comment written to the top of any stacker.yml that was generated
/// or rendered by the marketplace (rather than authored by the user). Its
/// presence flips `StackerConfig::origin` to `MarketplaceGenerated`, which
/// gates hook execution — see `HookPolicy` and `run_hook`.
///
/// The user can safely delete this comment after reviewing the file; once
/// gone, the file is treated as user-authored (trusted) again.
pub const MARKETPLACE_ORIGIN_MARKER: &str = "# @stacker-origin: marketplace";

/// Provenance of a `StackerConfig`. Used by hook execution to decide whether
/// to require the `--allow-untrusted-hooks` flag before running any shell
/// hook (`pre_build`, `post_deploy`, `on_failure`).
///
/// `UserAuthored` is the default because programmatic construction and
/// hand-written stacker.yml files represent code the user reviewed.
/// `MarketplaceGenerated` is set explicitly by `from_file`/`from_str` when
/// the file contains [`MARKETPLACE_ORIGIN_MARKER`], and by the marketplace
/// install command when it writes a fresh file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigOrigin {
    UserAuthored,
    MarketplaceGenerated,
}

impl Default for ConfigOrigin {
    fn default() -> Self {
        Self::UserAuthored
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Validate)]
pub struct StackerConfig {
    #[validate(min_length = 1)]
    #[validate(max_length = 128)]
    pub name: String,

    #[serde(default)]
    pub version: Option<String>,

    #[serde(default)]
    pub organization: Option<String>,

    #[serde(default)]
    pub project: ProjectConfig,

    #[serde(default)]
    pub app: AppSource,

    #[serde(default, deserialize_with = "deserialize_services")]
    pub services: Vec<ServiceDefinition>,

    #[serde(default)]
    pub proxy: ProxyConfig,

    #[serde(default)]
    pub deploy: DeployConfig,

    #[serde(default)]
    pub install: InstallConfig,

    #[serde(default)]
    pub environments: BTreeMap<String, EnvironmentConfig>,

    #[serde(default)]
    pub ai: AiConfig,

    #[serde(default, alias = "monitors")]
    pub monitoring: MonitoringConfig,

    #[serde(default)]
    pub hooks: HookConfig,

    #[serde(default)]
    pub env_file: Option<PathBuf>,

    #[serde(default)]
    pub env: HashMap<String, String>,

    #[serde(default)]
    pub config_contract: ConfigContract,

    /// Declaratively-defined pipes reconciled by `stacker pipe apply` / `diff`.
    /// Absent → empty, so existing configs are unaffected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pipes: Vec<PipeSpec>,

    /// Provenance of this config. Not serialized — computed at load time.
    ///
    /// Defaults to `UserAuthored`. `from_file`/`from_str` flip to
    /// `MarketplaceGenerated` if the raw text starts with
    /// [`MARKETPLACE_ORIGIN_MARKER`].
    #[serde(skip)]
    pub origin: ConfigOrigin,

    /// Whether the source config explicitly declared an `app:` section.
    ///
    /// `app` is a non-optional field with serde defaults, so a config that
    /// omits `app:` still deserializes to a default `AppSource`. This flag
    /// distinguishes "user/template actually declared an app to build" from
    /// "app was defaulted in". Set at load time by `from_file`/`from_str`
    /// (raw key present) and by `ConfigBuilder` (app setters used). The compose
    /// generator uses it to avoid synthesizing a phantom `app` service for
    /// services-only configs. Not serialized.
    #[serde(skip)]
    pub app_present: bool,
}

impl StackerConfig {
    /// Whether hooks in this config may be executed without an explicit
    /// `--allow-untrusted-hooks` flag.
    pub fn is_trusted(&self) -> bool {
        matches!(self.origin, ConfigOrigin::UserAuthored)
    }

    /// Load config from a file path, resolving `${VAR}` environment variable
    /// references and validating the result.
    ///
    /// Use this when you need the **resolved** values (e.g. for deployment,
    /// validation, or sending to the server).  If you plan to mutate the
    /// config and write it back to disk, use [`from_file_raw`] instead so
    /// that `${VAR}` placeholders are preserved.
    pub fn from_file(path: &Path) -> Result<Self, CliError> {
        Self::from_file_for_target(path, None)
    }

    /// Load config from a file path like [`from_file`], but skip `${VAR}`
    /// resolution inside whichever of `deploy.server` / `deploy.cloud` is
    /// **not** the active target — determined by `target_override` (e.g.
    /// the CLI's `--target` flag) or, absent that, the literal
    /// `deploy.target` value in the file.
    ///
    /// A project commonly defines both `deploy.server` and `deploy.cloud`
    /// (see the dual-target pattern) so it can be pointed at either without
    /// editing `stacker.yml`. Without this, `stacker deploy --target cloud`
    /// would fail on a missing `${EXISTING_SERVER_HOST}` even though the
    /// server section is never used for a cloud deploy — see GH #239.
    ///
    /// When the effective target can't be determined (no override, no
    /// literal `deploy.target` in the file, or a multi-target `deploy.targets`
    /// config), both sections are resolved as before — this only skips a
    /// section when we're confident it's inactive.
    pub fn from_file_for_target(
        path: &Path,
        target_override: Option<&str>,
    ) -> Result<Self, CliError> {
        if !path.exists() {
            return Err(CliError::ConfigNotFound {
                path: path.to_path_buf(),
            });
        }

        let raw_content = std::fs::read_to_string(path)?;
        let origin = detect_origin_from_raw(&raw_content);
        let mut parsed: serde_yaml::Value = serde_yaml::from_str(&raw_content)?;
        let env_file_vars = load_env_file_vars_from_yaml(path, &raw_content);

        let (skip_server, skip_cloud) = inactive_deploy_sections(&parsed, target_override);
        resolve_env_placeholders_in_value_skipping_deploy(
            &mut parsed,
            &env_file_vars,
            skip_server,
            skip_cloud,
        )?;
        let app_present = parsed.get("app").is_some();
        let mut config = deserialize_config_value(parsed)?;
        config.origin = origin;
        config.app_present = app_present;
        Ok(config)
    }

    /// Load config from a file path **without** resolving `${VAR}` placeholders.
    ///
    /// Use this when you need to modify the config and write it back to disk
    /// (e.g. `stacker service add`, `stacker config fix`).  The `${VAR}`
    /// references are kept as-is so they are not replaced with sensitive
    /// values when the file is serialized back.
    pub fn from_file_raw(path: &Path) -> Result<Self, CliError> {
        if !path.exists() {
            return Err(CliError::ConfigNotFound {
                path: path.to_path_buf(),
            });
        }

        let raw_content = std::fs::read_to_string(path)?;
        let origin = detect_origin_from_raw(&raw_content);
        let parsed: serde_yaml::Value = serde_yaml::from_str(&raw_content)?;
        let app_present = parsed.get("app").is_some();
        let mut config = deserialize_config_value(parsed)?;
        config.origin = origin;
        config.app_present = app_present;
        Ok(config)
    }

    /// Load config from a YAML string **without** resolving `${VAR}` placeholders.
    ///
    /// Use this for validation/preview where referenced variables (e.g.
    /// config_contract install inputs) are not yet known. `${VAR}` references
    /// are kept as-is so a missing variable does not fail the parse.
    pub fn from_str_raw(yaml: &str) -> Result<Self, CliError> {
        let origin = detect_origin_from_raw(yaml);
        let parsed: serde_yaml::Value = serde_yaml::from_str(yaml)?;
        let app_present = parsed.get("app").is_some();
        let mut config = deserialize_config_value(parsed)?;
        config.origin = origin;
        config.app_present = app_present;
        Ok(config)
    }

    /// Load config from a YAML string (useful for tests).
    pub fn from_str(yaml: &str) -> Result<Self, CliError> {
        let origin = detect_origin_from_raw(yaml);
        let mut parsed: serde_yaml::Value = serde_yaml::from_str(yaml)?;
        resolve_env_placeholders_in_value(&mut parsed, &HashMap::new())?;
        let app_present = parsed.get("app").is_some();
        let mut config = deserialize_config_value(parsed)?;
        config.origin = origin;
        config.app_present = app_present;
        Ok(config)
    }

    /// Return a cloned config with `deploy` flattened to one selected target.
    ///
    /// Legacy configs keep working as before. Multi-target configs resolve one
    /// named profile into the existing single-target fields.
    pub fn with_resolved_deploy_target(&self, requested: Option<&str>) -> Result<Self, CliError> {
        let mut config = self.clone();
        config.deploy = self.deploy.resolve(requested)?;
        Ok(config)
    }

    pub fn selected_environment(&self, override_environment: Option<&str>) -> Option<String> {
        override_environment
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| self.deploy.environment.clone())
    }

    pub fn resolve_environment_config(
        &self,
        override_environment: Option<&str>,
    ) -> Result<Option<(String, EnvironmentConfig)>, CliError> {
        let Some(environment) = self.selected_environment(override_environment) else {
            return Ok(None);
        };

        let configured = self.environments.get(&environment).cloned();
        let compose_file = configured
            .as_ref()
            .and_then(|config| config.compose_file.clone())
            .or_else(|| self.deploy.compose_file.clone())
            .or_else(|| Some(PathBuf::from(format!("docker/{environment}/compose.yml"))));
        let env_file = configured
            .as_ref()
            .and_then(|config| config.env_file.clone())
            .or_else(|| self.env_file.clone());

        Ok(Some((
            environment,
            EnvironmentConfig {
                compose_file,
                env_file,
            },
        )))
    }

    /// Validate cross-field semantic constraints beyond serde deserialization.
    /// Returns a list of issues (errors, warnings, info).
    pub fn validate_semantics(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        if self.deploy.uses_named_targets() {
            if self.deploy.targets.len() > 1
                && self
                    .deploy
                    .default_target
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
            {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    code: "E004".to_string(),
                    message: "deploy.default_target is required when deploy.targets defines multiple entries".to_string(),
                    field: Some("deploy.default_target".to_string()),
                });
            }

            if let Some(default_target) = self
                .deploy
                .default_target
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if !self.deploy.targets.contains_key(default_target) {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        code: "E005".to_string(),
                        message: format!(
                            "deploy.default_target '{}' does not match any entry in deploy.targets",
                            default_target
                        ),
                        field: Some("deploy.default_target".to_string()),
                    });
                }
            }

            for (name, profile) in &self.deploy.targets {
                let field_prefix = format!("deploy.targets.{name}");
                match profile.inferred_target(name) {
                    Ok(target) => {
                        let deploy = DeployConfig {
                            target,
                            environment: profile.environment.clone(),
                            compose_file: profile.compose_file.clone(),
                            deployment_hash: profile.deployment_hash.clone(),
                            cloud: profile.cloud.clone(),
                            server: profile.server.clone(),
                            registry: profile.registry.clone(),
                            default_target: None,
                            targets: BTreeMap::new(),
                        };
                        validate_deploy_semantics(
                            &mut issues,
                            &self.project,
                            &deploy,
                            Some(field_prefix),
                        );
                    }
                    Err(_) => issues.push(ValidationIssue {
                        severity: Severity::Error,
                        code: "E006".to_string(),
                        message: format!(
                            "deploy.targets.{name} cannot define both 'server' and 'cloud'"
                        ),
                        field: Some(field_prefix),
                    }),
                }
            }
        } else {
            validate_deploy_semantics(
                &mut issues,
                &self.project,
                &self.deploy,
                Some("deploy".into()),
            );
        }

        // Custom app type with no image and no dockerfile
        if self.app.app_type == AppType::Custom
            && self.app.image.is_none()
            && self.app.dockerfile.is_none()
        {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                code: "E003".to_string(),
                message: "Custom app type requires either 'image' or 'dockerfile'".to_string(),
                field: Some("app".to_string()),
            });
        }

        // E007 — every port mapping must be a real port. Checked here so a bad
        // value fails at `stacker config validate` rather than on the target
        // host after the server is already provisioned.
        for (field_name, port_str) in self
            .app
            .ports
            .iter()
            .map(|p| ("app.ports".to_string(), p))
            .chain(self.services.iter().flat_map(|svc| {
                svc.ports
                    .iter()
                    .map(move |p| (format!("services.{}.ports", svc.name), p))
            }))
        {
            if let Err(err) = validate_port_mapping(port_str) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    code: "E007".to_string(),
                    message: format!("Invalid port mapping '{port_str}': {err}."),
                    field: Some(field_name),
                });
            }
        }

        // Port conflict detection across services, keyed by the *published
        // host port*. Only services that publish a fixed host port can collide;
        // the container-only form (no host port) is skipped. `host_port_binding`
        // correctly handles the `ip:host:container` form — extracting the host
        // port, not the leading IP — so two loopback services on different
        // ports (e.g. `127.0.0.1:5432:5432` and `127.0.0.1:6379:6379`) are no
        // longer misreported as sharing a port.
        let mut port_map: HashMap<String, Vec<String>> = HashMap::new();
        for svc in &self.services {
            for port_str in &svc.ports {
                if let Some(host_port) = host_port_binding(port_str) {
                    port_map
                        .entry(host_port)
                        .or_default()
                        .push(svc.name.clone());
                }
            }
        }
        for (port, services) in &port_map {
            if services.len() > 1 {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    code: "W001".to_string(),
                    message: format!(
                        "Port {} is used by multiple services: {}",
                        port,
                        services.join(", ")
                    ),
                    field: Some("services.ports".to_string()),
                });
            }
        }

        // W003 — a `proxy:` block deploys a *platform-managed* reverse proxy
        // that owns the ingress host ports on the target. Any app/service that
        // also publishes one of those host ports collides with it: the managed
        // proxy takes precedence and the user's binding is shadowed. Detect by
        // host-port overlap (not by image name) so it also catches a plain app
        // accidentally bound to :80, not only a rival proxy.
        if self.proxy.proxy_type != ProxyType::None {
            let ingress_ports: &[&str] = match self.proxy.proxy_type {
                ProxyType::NginxProxyManager => &["80", "443", "81"],
                ProxyType::Nginx | ProxyType::Traefik | ProxyType::Caddy => &["80", "443"],
                ProxyType::None => &[],
            };

            let mut published: Vec<(String, String)> = Vec::new();
            for port in &self.app.ports {
                if let Some(host_port) = host_port_binding(port) {
                    published.push(("app".to_string(), host_port));
                }
            }
            for svc in &self.services {
                for port in &svc.ports {
                    if let Some(host_port) = host_port_binding(port) {
                        published.push((svc.name.clone(), host_port));
                    }
                }
            }

            let mut conflicts: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (service, host_port) in published {
                if ingress_ports.contains(&host_port.as_str()) {
                    conflicts.entry(service).or_default().push(host_port);
                }
            }
            for (service, ports) in conflicts {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    code: "W003".to_string(),
                    message: format!(
                        "Service '{service}' publishes host port(s) {} that the platform-managed \
                         '{}' proxy (configured via the proxy: block) also claims on the deploy \
                         target. The managed proxy takes precedence, so '{service}' is ignored on \
                         those ports. Remove the proxy: block to keep your own service there, or \
                         change its host port.",
                        ports.join(", "),
                        self.proxy.proxy_type
                    ),
                    field: Some("proxy.type".to_string()),
                });
            }
        }

        issues
    }
}

/// Inspect the raw text of a stacker.yml (or a `from_str` input) and decide
/// whether it was authored by the user or generated by the marketplace.
///
/// A file counts as marketplace-generated if any of the leading comment
/// lines contains [`MARKETPLACE_ORIGIN_MARKER`]. Blank lines are skipped so
/// the marker can appear after a shebang-style banner. Once the scan hits
/// a non-comment non-blank line, further lines are ignored — the marker
/// must live at the top of the file.
fn detect_origin_from_raw(raw: &str) -> ConfigOrigin {
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            if rest
                .trim()
                .eq_ignore_ascii_case(MARKETPLACE_ORIGIN_MARKER.trim_start_matches('#').trim())
            {
                return ConfigOrigin::MarketplaceGenerated;
            }
            continue;
        }
        break;
    }
    ConfigOrigin::UserAuthored
}

fn deserialize_config_value(parsed: serde_yaml::Value) -> Result<StackerConfig, CliError> {
    let rendered = serde_yaml::to_string(&parsed)?;
    let deserializer = serde_yaml::Deserializer::from_str(&rendered);

    serde_path_to_error::deserialize::<_, StackerConfig>(deserializer).map_err(|err| {
        let field_path = err.path().to_string();
        let source = err.into_inner();
        let message = format_config_parse_message(&field_path, &source);
        CliError::ConfigParseFailed {
            source: <serde_yaml::Error as serde::de::Error>::custom(message),
        }
    })
}

fn format_config_parse_message(field_path: &str, source: &serde_yaml::Error) -> String {
    let source_message = source.to_string();
    let normalized_field = if field_path.is_empty() || field_path == "." {
        None
    } else {
        Some(field_path)
    };

    if let Some(field) = normalized_field {
        if source_message.contains("expected path string") {
            let example = if field == "app.path" {
                "`.` or `./app`"
            } else {
                "`./path/to/file`"
            };

            if source_message.contains("invalid type: unit value") {
                return format!(
                    "invalid empty path at `{field}`. Remove the key or set it to a quoted path string like {example}"
                );
            }

            return format!(
                "invalid path at `{field}`. Expected a quoted path string like {example}. Original parser error: {source_message}"
            );
        }

        return format!("invalid value at `{field}`: {source_message}");
    }

    source_message
}

fn validate_deploy_semantics(
    issues: &mut Vec<ValidationIssue>,
    project: &ProjectConfig,
    deploy: &DeployConfig,
    field_prefix: Option<String>,
) {
    let field = |suffix: &str| -> String {
        match &field_prefix {
            Some(prefix) => format!("{prefix}.{suffix}"),
            None => suffix.to_string(),
        }
    };

    if deploy.target == DeployTarget::Cloud && deploy.cloud.is_none() {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            code: "E001".to_string(),
            message: "Cloud provider configuration is required for cloud deployment".to_string(),
            field: Some(field("cloud.provider")),
        });
    }

    if deploy.target == DeployTarget::Server && deploy.server.is_none() {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            code: "E002".to_string(),
            message: "Server host is required for server deployment".to_string(),
            field: Some(field("server.host")),
        });
    }

    if deploy.target == DeployTarget::Cloud {
        if let Some(cloud) = &deploy.cloud {
            if cloud.orchestrator == CloudOrchestrator::Remote {
                let identity_empty = project
                    .identity
                    .as_ref()
                    .map(|v| v.trim().is_empty())
                    .unwrap_or(true);

                if identity_empty {
                    issues.push(ValidationIssue {
                        severity: Severity::Info,
                        code: "I001".to_string(),
                        message: "project.identity is not set; remote deploy will use default stack_code 'custom-stack'".to_string(),
                        field: Some("project.identity".to_string()),
                    });
                }
            }

            // Validate public_ports format up front so invalid entries are
            // surfaced by `stacker config validate` instead of being silently
            // dropped during cloud firewall provisioning. Bare numbers and
            // "port/proto" specs are both accepted.
            for port in &cloud.public_ports {
                if let Err(err) = crate::forms::firewall::normalize_public_port(port) {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        code: "E005".to_string(),
                        message: format!(
                            "Invalid public_ports entry '{}': {}. Expected a port number or \"port/protocol\" (e.g. \"8000\" or \"8000/tcp\").",
                            port, err
                        ),
                        field: Some(field("cloud.public_ports")),
                    });
                }
            }
        }
    }
}

fn load_env_file_vars_from_yaml(path: &Path, raw_content: &str) -> HashMap<String, String> {
    let parsed: serde_yaml::Value = match serde_yaml::from_str(raw_content) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };

    let env_file_value = parsed
        .get("env_file")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let env_file = match env_file_value {
        Some(v) => v,
        None => return HashMap::new(),
    };

    let config_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let env_file_path = Path::new(env_file);
    let resolved_path = if env_file_path.is_absolute() {
        env_file_path.to_path_buf()
    } else {
        config_dir.join(env_file_path)
    };

    let content = match std::fs::read_to_string(&resolved_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let mut vars = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            if key.is_empty() {
                continue;
            }

            let mut value = value.trim().to_string();
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                if value.len() >= 2 {
                    value = value[1..value.len() - 1].to_string();
                }
            }
            vars.insert(key.to_string(), value);
        }
    }

    vars
}

/// Extract the host port from a port mapping string like "8080:80" → "8080".
/// Extract the *published host* port from a compose port spec, or `None` when
/// the spec publishes no host port (container-only form). Handles the protocol
/// suffix and all three binding forms:
///   `"80"` -> None (ephemeral), `"80:80"` -> `"80"`,
///   `"127.0.0.1:80:80"` -> `"80"`, `"80:80/tcp"` -> `"80"`.
/// Validate one numeric port, rejecting anything outside Docker's 1-65535.
fn validate_port_number(value: &str) -> Result<(), String> {
    let parsed: u64 = value
        .parse()
        .map_err(|_| format!("'{value}' is not a port number"))?;
    if parsed == 0 || parsed > u16::MAX as u64 {
        return Err(format!("port {value} is out of range, must be 1-{}", u16::MAX));
    }
    Ok(())
}

/// Validate one side of a mapping, which may be a range (`8000-8010`).
fn validate_port_segment(segment: &str) -> Result<(), String> {
    match segment.split_once('-') {
        Some((low, high)) => {
            validate_port_number(low)?;
            validate_port_number(high)
        }
        None => validate_port_number(segment),
    }
}

/// Validate a docker-compose short-form port mapping.
///
/// Accepts every shape Compose does — `container`, `host:container`,
/// `ip:host:container` (including bracketed IPv6), ranges on either side, and
/// an optional `/tcp`, `/udp` or `/sctp` suffix.
///
/// Without this, `app.ports` and `services[].ports` reached the generated
/// compose verbatim and were only rejected on the target host, *after*
/// provisioning: a bare out-of-range entry like `"133342"` is read by Docker
/// as a container port and fails the deploy with "invalid containerPort".
/// `deploy.cloud.public_ports` was already checked (E005); these were not.
fn validate_port_mapping(spec: &str) -> Result<(), String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("port mapping is empty".to_string());
    }

    let (mapping, protocol) = match spec.rsplit_once('/') {
        Some((mapping, proto)) => (mapping, Some(proto)),
        None => (spec, None),
    };

    if let Some(proto) = protocol {
        if !matches!(proto, "tcp" | "udp" | "sctp") {
            return Err(format!(
                "unknown protocol '{proto}', expected tcp, udp or sctp"
            ));
        }
    }

    // A bracketed IPv6 host IP (`[::1]:8080:80`) is full of colons, so strip it
    // before splitting on `:` — otherwise every hextet looks like a port.
    let mapping = match mapping.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((_ip, remainder)) => remainder.strip_prefix(':').unwrap_or(remainder),
            None => return Err(format!("'{spec}' has an unterminated IPv6 address")),
        },
        None => mapping,
    };

    let parts: Vec<&str> = mapping.split(':').collect();
    let port_parts: &[&str] = match parts.len() {
        // "container", "host:container"
        1 | 2 => &parts,
        // "ip:host:container" — the leading segment is a host IP, not a port.
        3 => &parts[1..],
        _ => return Err(format!("'{spec}' is not a valid port mapping")),
    };

    for segment in port_parts {
        validate_port_segment(segment)?;
    }

    Ok(())
}

fn host_port_binding(port_str: &str) -> Option<String> {
    let spec = port_str.split('/').next().unwrap_or(port_str);
    let parts: Vec<&str> = spec.split(':').collect();
    match parts.as_slice() {
        [_container] => None,
        [host, _container] => Some((*host).to_string()),
        [_ip, host, _container] => Some((*host).to_string()),
        _ => None,
    }
}

/// Resolve `${VAR_NAME}` references in a string using process environment.
#[allow(dead_code)]
fn resolve_env_vars(content: &str) -> Result<String, CliError> {
    resolve_env_vars_with_fallback(content, &HashMap::new())
}

/// Decide which of `deploy.server` / `deploy.cloud` (legacy single-block
/// form) is inactive for the effective target, so env-var resolution can
/// skip it entirely. Returns `(skip_server, skip_cloud)`.
///
/// Only acts when the effective target is confidently known — from
/// `target_override` (e.g. `--target`) or a literal (non-templated)
/// `deploy.target` in the file. Multi-target `deploy.targets.<name>`
/// configs are left untouched: each named profile already carries only
/// its own `server` or `cloud`, so there's no ambiguity to resolve.
fn inactive_deploy_sections(
    parsed: &serde_yaml::Value,
    target_override: Option<&str>,
) -> (bool, bool) {
    let Some(root) = parsed.as_mapping() else {
        return (false, false);
    };
    let Some(deploy) = root
        .get(serde_yaml::Value::String("deploy".to_string()))
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return (false, false);
    };

    let has_named_targets = deploy
        .get(serde_yaml::Value::String("targets".to_string()))
        .and_then(serde_yaml::Value::as_mapping)
        .map(|m| !m.is_empty())
        .unwrap_or(false);
    if has_named_targets {
        return (false, false);
    }

    let effective_target = target_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .or_else(|| {
            deploy
                .get(serde_yaml::Value::String("target".to_string()))
                .and_then(serde_yaml::Value::as_str)
                .filter(|s| !s.contains("${")) // don't trust an unresolved literal
                .map(|s| s.trim().to_lowercase())
        });

    match effective_target.as_deref() {
        Some("cloud") => (true, false),
        Some("server") => (false, true),
        Some("local") => (true, true),
        _ => (false, false),
    }
}

/// Like [`resolve_env_placeholders_in_value`], but leaves `deploy.server`
/// and/or `deploy.cloud` completely untouched (placeholders and all) when
/// `skip_server`/`skip_cloud` say that section is inactive for this deploy.
fn resolve_env_placeholders_in_value_skipping_deploy(
    value: &mut serde_yaml::Value,
    fallback_vars: &HashMap<String, String>,
    skip_server: bool,
    skip_cloud: bool,
) -> Result<(), CliError> {
    if !skip_server && !skip_cloud {
        return resolve_env_placeholders_in_value(value, fallback_vars);
    }

    let Some(root) = value.as_mapping_mut() else {
        return resolve_env_placeholders_in_value(value, fallback_vars);
    };

    for (key, map_value) in root.iter_mut() {
        if key.as_str() != Some("deploy") {
            resolve_env_placeholders_in_value(map_value, fallback_vars)?;
            continue;
        }
        let Some(deploy_map) = map_value.as_mapping_mut() else {
            continue;
        };
        for (deploy_key, deploy_value) in deploy_map.iter_mut() {
            let skip = (deploy_key.as_str() == Some("server") && skip_server)
                || (deploy_key.as_str() == Some("cloud") && skip_cloud);
            if skip {
                continue;
            }
            resolve_env_placeholders_in_value(deploy_value, fallback_vars)?;
        }
    }

    Ok(())
}

fn resolve_env_placeholders_in_value(
    value: &mut serde_yaml::Value,
    fallback_vars: &HashMap<String, String>,
) -> Result<(), CliError> {
    match value {
        serde_yaml::Value::String(raw) => {
            let resolved = resolve_env_vars_with_fallback(raw, fallback_vars)?;
            *raw = resolved;
        }
        serde_yaml::Value::Sequence(items) => {
            for item in items.iter_mut() {
                resolve_env_placeholders_in_value(item, fallback_vars)?;
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for (_key, map_value) in map.iter_mut() {
                resolve_env_placeholders_in_value(map_value, fallback_vars)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn resolve_env_vars_with_fallback(
    content: &str,
    fallback_vars: &HashMap<String, String>,
) -> Result<String, CliError> {
    let mut result = content.to_string();
    let re = regex::Regex::new(r"\$\{([^}]+)\}").expect("valid regex");

    // Collect all matches first to avoid borrow issues
    let captures: Vec<(String, String)> = re
        .captures_iter(content)
        .map(|cap| {
            let full_match = cap[0].to_string();
            let var_name = cap[1].to_string();
            (full_match, var_name)
        })
        .collect();

    for (full_match, var_name) in captures {
        let value =
            match std::env::var(&var_name) {
                Ok(v) => v,
                Err(_) => fallback_vars.get(&var_name).cloned().ok_or_else(|| {
                    CliError::EnvVarNotFound {
                        var_name: var_name.clone(),
                    }
                })?,
            };
        result = result.replace(&full_match, &value);
    }

    Ok(result)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ConfigBuilder — fluent builder for programmatic construction
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Default)]
pub struct ConfigBuilder {
    name: Option<String>,
    version: Option<String>,
    organization: Option<String>,
    project_identity: Option<String>,
    app_type: Option<AppType>,
    app_path: Option<PathBuf>,
    app_image: Option<String>,
    app_dockerfile: Option<PathBuf>,
    app_volumes: Vec<String>,
    app_ports: Vec<String>,
    build_args: HashMap<String, String>,
    services: Vec<ServiceDefinition>,
    proxy: Option<ProxyConfig>,
    deploy_target: Option<DeployTarget>,
    deployment_hash: Option<String>,
    cloud: Option<CloudConfig>,
    server: Option<ServerConfig>,
    registry: Option<RegistryConfig>,
    ai: Option<AiConfig>,
    monitoring: Option<MonitoringConfig>,
    hooks: Option<HookConfig>,
    env: HashMap<String, String>,
    env_file: Option<PathBuf>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn version<S: Into<String>>(mut self, version: S) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn organization<S: Into<String>>(mut self, org: S) -> Self {
        self.organization = Some(org.into());
        self
    }

    pub fn project_identity<S: Into<String>>(mut self, identity: S) -> Self {
        self.project_identity = Some(identity.into());
        self
    }

    pub fn app_type(mut self, app_type: AppType) -> Self {
        self.app_type = Some(app_type);
        self
    }

    pub fn app_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.app_path = Some(path.into());
        self
    }

    pub fn app_image<S: Into<String>>(mut self, image: S) -> Self {
        self.app_image = Some(image.into());
        self
    }

    pub fn app_dockerfile<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.app_dockerfile = Some(path.into());
        self
    }

    pub fn app_ports(mut self, ports: Vec<String>) -> Self {
        self.app_ports = ports;
        self
    }

    pub fn app_volumes(mut self, volumes: Vec<String>) -> Self {
        self.app_volumes = volumes;
        self
    }

    pub fn build_arg<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.build_args.insert(key.into(), value.into());
        self
    }

    pub fn add_service(mut self, service: ServiceDefinition) -> Self {
        self.services.push(service);
        self
    }

    pub fn proxy(mut self, proxy: ProxyConfig) -> Self {
        self.proxy = Some(proxy);
        self
    }

    pub fn deploy_target(mut self, target: DeployTarget) -> Self {
        self.deploy_target = Some(target);
        self
    }

    pub fn deployment_hash<S: Into<String>>(mut self, hash: S) -> Self {
        self.deployment_hash = Some(hash.into());
        self
    }

    pub fn cloud(mut self, cloud: CloudConfig) -> Self {
        self.cloud = Some(cloud);
        self
    }

    pub fn server(mut self, server: ServerConfig) -> Self {
        self.server = Some(server);
        self
    }

    pub fn registry(mut self, registry: RegistryConfig) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn ai(mut self, ai: AiConfig) -> Self {
        self.ai = Some(ai);
        self
    }

    pub fn monitoring(mut self, monitoring: MonitoringConfig) -> Self {
        self.monitoring = Some(monitoring);
        self
    }

    pub fn hooks(mut self, hooks: HookConfig) -> Self {
        self.hooks = Some(hooks);
        self
    }

    pub fn env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn env_file<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.env_file = Some(path.into());
        self
    }

    /// Consume the builder, validate required fields, and produce StackerConfig.
    pub fn build(self) -> Result<StackerConfig, CliError> {
        let name = self
            .name
            .ok_or_else(|| CliError::ConfigValidation("name is required".into()))?;

        // Any app-related builder setter marks the app as explicitly declared,
        // so the compose generator materializes it even alongside services.
        let app_present = self.app_type.is_some()
            || self.app_image.is_some()
            || self.app_dockerfile.is_some()
            || !self.app_volumes.is_empty()
            || !self.app_ports.is_empty()
            || !self.build_args.is_empty();

        let build_config = if self.build_args.is_empty() {
            None
        } else {
            Some(BuildConfig {
                context: ".".to_string(),
                args: self.build_args,
            })
        };

        Ok(StackerConfig {
            name,
            version: self.version,
            organization: self.organization,
            project: ProjectConfig {
                identity: self.project_identity,
            },
            app: AppSource {
                app_type: self.app_type.unwrap_or_default(),
                path: self.app_path.unwrap_or_else(|| PathBuf::from(".")),
                dockerfile: self.app_dockerfile,
                image: self.app_image,
                build: build_config,
                ports: self.app_ports,
                volumes: self.app_volumes,
                environment: HashMap::new(),
                command: None,
                healthcheck: None,
            },
            services: self.services,
            proxy: self.proxy.unwrap_or_default(),
            deploy: DeployConfig {
                target: self.deploy_target.unwrap_or_default(),
                environment: None,
                compose_file: None,
                deployment_hash: self.deployment_hash,
                cloud: self.cloud,
                server: self.server,
                registry: self.registry,
                default_target: None,
                targets: BTreeMap::new(),
            },
            install: InstallConfig::default(),
            environments: BTreeMap::new(),
            ai: self.ai.unwrap_or_default(),
            monitoring: self.monitoring.unwrap_or_default(),
            hooks: self.hooks.unwrap_or_default(),
            env_file: self.env_file,
            env: self.env,
            config_contract: ConfigContract::default(),
            pipes: Vec::new(),
            origin: ConfigOrigin::UserAuthored,
            app_present,
        })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests — Phase 1: Config parser + builder
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn field_policy_legacy_required_list_maps_to_fixed_required() {
        let yaml = r#"
name: my-site
config_contract:
  services:
    auth:
      required:
        - POSTGRES_HOST
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        let auth = &config.config_contract.services["auth"];
        assert_eq!(auth.required_keys(), vec!["POSTGRES_HOST".to_string()]);
        let policy = &auth.fields["POSTGRES_HOST"];
        assert_eq!(policy.mutability, Mutability::Fixed);
        assert!(policy.required);
    }

    #[test]
    fn field_policy_legacy_optional_list_maps_to_fixed_not_required() {
        let yaml = r#"
name: my-site
config_contract:
  services:
    auth:
      optional:
        - LOG_LEVEL
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        let auth = &config.config_contract.services["auth"];
        assert_eq!(auth.optional_keys(), vec!["LOG_LEVEL".to_string()]);
        let policy = &auth.fields["LOG_LEVEL"];
        assert_eq!(policy.mutability, Mutability::Fixed);
        assert!(!policy.required);
    }

    #[test]
    fn field_policy_legacy_secret_list_maps_to_generated_default() {
        let yaml = r#"
name: my-site
config_contract:
  services:
    auth:
      secret:
        - JWT_SECRET
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        let auth = &config.config_contract.services["auth"];
        assert_eq!(auth.secret_keys(), vec!["JWT_SECRET".to_string()]);
        let policy = &auth.fields["JWT_SECRET"];
        assert_eq!(policy.mutability, Mutability::Generated);
        assert!(policy.required);
        assert_eq!(policy.type_spec, Some(FieldType::Alphanumeric));
        assert_eq!(policy.min_length, Some(32));
    }

    #[test]
    fn field_policy_full_form_parses_hex_editable_and_derived_jwt() {
        let yaml = r#"
name: my-site
config_contract:
  services:
    auth:
      fields:
        JWT_SECRET:
          mutability: generated
          type: hex
          length: 32
        LOG_LEVEL:
          mutability: editable
          required: false
          type: enum
          values: [debug, info, warn, error]
    storage:
      fields:
        ANON_KEY:
          mutability: generated
          type: derived_jwt
          signing_key: auth.JWT_SECRET
          claims:
            role: anon
            iss: supabase
          alg: HS256
"#;
        let config = StackerConfig::from_str(yaml).unwrap();

        let auth = &config.config_contract.services["auth"];
        let jwt_secret = &auth.fields["JWT_SECRET"];
        assert_eq!(jwt_secret.mutability, Mutability::Generated);
        assert_eq!(jwt_secret.type_spec, Some(FieldType::Hex));
        assert_eq!(jwt_secret.length, Some(32));

        let log_level = &auth.fields["LOG_LEVEL"];
        assert_eq!(log_level.mutability, Mutability::Editable);
        assert!(!log_level.required);
        assert_eq!(
            log_level.values,
            vec!["debug", "info", "warn", "error"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(auth.editable_keys(), vec!["LOG_LEVEL".to_string()]);

        let storage = &config.config_contract.services["storage"];
        let anon_key = &storage.fields["ANON_KEY"];
        assert_eq!(anon_key.mutability, Mutability::Generated);
        assert_eq!(anon_key.type_spec, Some(FieldType::DerivedJwt));
        assert_eq!(anon_key.signing_key.as_deref(), Some("auth.JWT_SECRET"));
        assert_eq!(anon_key.alg.as_deref(), Some("HS256"));
    }

    #[test]
    fn field_policy_generated_without_type_is_rejected() {
        let yaml = r#"
name: my-site
config_contract:
  services:
    auth:
      fields:
        JWT_SECRET:
          mutability: generated
"#;
        let err = StackerConfig::from_str(yaml).unwrap_err();
        assert!(format!("{err}").contains("must declare a type"));
    }

    #[test]
    fn field_policy_derived_jwt_missing_signing_key_is_rejected() {
        let yaml = r#"
name: my-site
config_contract:
  services:
    storage:
      fields:
        ANON_KEY:
          mutability: generated
          type: derived_jwt
          claims:
            role: anon
          alg: HS256
"#;
        let err = StackerConfig::from_str(yaml).unwrap_err();
        assert!(format!("{err}").contains("derived_jwt"));
    }

    #[test]
    fn field_policy_unknown_mutability_is_rejected() {
        let yaml = r#"
name: my-site
config_contract:
  services:
    auth:
      fields:
        SOME_FIELD:
          mutability: readonly
"#;
        assert!(StackerConfig::from_str(yaml).is_err());
    }

    #[test]
    fn field_policy_round_trips_through_legacy_yaml_shape() {
        let contract = TargetConfigContract::from_legacy_lists(
            vec!["POSTGRES_HOST".to_string()],
            vec!["LOG_LEVEL".to_string()],
            vec!["JWT_SECRET".to_string()],
        );
        let yaml = serde_yaml::to_string(&contract).unwrap();
        assert!(yaml.contains("required:"));
        assert!(yaml.contains("- POSTGRES_HOST"));
        assert!(yaml.contains("optional:"));
        assert!(yaml.contains("- LOG_LEVEL"));
        assert!(yaml.contains("secret:"));
        assert!(yaml.contains("- JWT_SECRET"));
        assert!(!yaml.contains("fields:"));

        let reparsed: TargetConfigContract = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(reparsed.required_keys(), contract.required_keys());
        assert_eq!(reparsed.optional_keys(), contract.optional_keys());
        assert_eq!(reparsed.secret_keys(), contract.secret_keys());
    }

    #[test]
    fn test_parse_minimal_config() {
        let yaml = r#"
name: my-site
app:
  type: static
  path: ./public
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.name, "my-site");
        assert_eq!(config.app.app_type, AppType::Static);
        assert_eq!(config.app.path, PathBuf::from("./public"));
        assert!(config.services.is_empty());
        assert_eq!(config.proxy.proxy_type, ProxyType::None);
        assert_eq!(config.deploy.target, DeployTarget::Local);
        assert!(!config.ai.enabled);
        assert!(!config.monitoring.status_panel);
    }

    #[test]
    fn test_parse_install_inputs() {
        let yaml = r#"
name: wordpress-site
install:
  inputs:
    commonDomain: example.com
    admin_email: admin@example.com
deploy:
  target: cloud
"#;

        let config = StackerConfig::from_str(yaml).unwrap();

        assert_eq!(
            config.install.inputs.get("commonDomain"),
            Some(&serde_json::json!("example.com"))
        );
        assert_eq!(
            config.install.inputs.get("admin_email"),
            Some(&serde_json::json!("admin@example.com"))
        );
    }

    #[test]
    fn test_parse_full_config() {
        let yaml = r#"
name: full-app
version: "2.0"
organization: test-org
app:
  type: node
  path: ./src
  build:
    context: .
    args:
      NODE_ENV: production
services:
  - name: postgres
    image: postgres:16
    ports: ["5432:5432"]
    environment:
      POSTGRES_DB: testdb
  - name: redis
    image: redis:7-alpine
    ports: ["6379:6379"]
proxy:
  type: nginx
  domains:
    - domain: test.example.com
      ssl: auto
      upstream: app:3000
deploy:
  target: local
ai:
  enabled: true
  provider: ollama
  model: llama3
  endpoint: http://localhost:11434
  tasks: [dockerfile, troubleshoot]
monitoring:
  status_panel: true
  healthcheck:
    endpoint: /health
    interval: 30s
env:
  APP_PORT: "3000"
  LOG_LEVEL: debug
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.name, "full-app");
        assert_eq!(config.version, Some("2.0".to_string()));
        assert_eq!(config.organization, Some("test-org".to_string()));
        assert_eq!(config.app.app_type, AppType::Node);
        assert_eq!(config.services.len(), 2);
        assert_eq!(config.services[0].name, "postgres");
        assert_eq!(config.services[1].name, "redis");
        assert_eq!(config.proxy.proxy_type, ProxyType::Nginx);
        assert_eq!(config.proxy.domains.len(), 1);
        assert_eq!(config.proxy.domains[0].domain, "test.example.com");
        assert_eq!(config.proxy.domains[0].ssl, SslMode::Auto);
        assert!(config.ai.enabled);
        assert_eq!(config.ai.provider, AiProviderType::Ollama);
        assert!(config.monitoring.status_panel);
        assert_eq!(config.env.get("APP_PORT").unwrap(), "3000");
    }

    #[test]
    fn test_parse_multi_target_config_and_resolve_default() {
        let yaml = r#"
name: multi-target-app
app:
  type: static
deploy:
  default_target: dev-server
  targets:
    local:
      compose_file: docker/local/compose.yml
    dev-server:
      server:
        host: 10.0.0.8
        user: deploy
        ssh_key: ~/.ssh/id_ed25519
"#;

        let config = StackerConfig::from_str(yaml).unwrap();
        assert!(config.deploy.uses_named_targets());
        assert_eq!(config.deploy.targets.len(), 2);

        let resolved = config.with_resolved_deploy_target(None).unwrap();
        assert_eq!(resolved.deploy.target, DeployTarget::Server);
        assert!(resolved.deploy.environment.is_none());
        assert_eq!(
            resolved
                .deploy
                .server
                .as_ref()
                .map(|server| server.host.as_str()),
            Some("10.0.0.8")
        );
    }

    #[test]
    fn test_resolve_named_target_override() {
        let yaml = r#"
name: multi-target-app
app:
  type: static
deploy:
  default_target: local
  targets:
    local:
      compose_file: docker/local/compose.yml
    prod:
      cloud:
        provider: aws
"#;

        let config = StackerConfig::from_str(yaml).unwrap();
        let resolved = config.with_resolved_deploy_target(Some("prod")).unwrap();

        assert_eq!(resolved.deploy.target, DeployTarget::Cloud);
        assert_eq!(
            resolved.deploy.cloud.as_ref().map(|cloud| cloud.provider),
            Some(CloudProvider::Aws)
        );
        assert!(resolved.deploy.compose_file.is_none());
    }

    #[test]
    fn test_parse_environment_config_and_default_selection() {
        let yaml = r#"
name: environment-app
app:
  type: static
deploy:
  target: cloud
  environment: production
environments:
  production:
    compose_file: docker/production/compose.yml
    env_file: docker/production/.env
"#;

        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.deploy.environment.as_deref(), Some("production"));
        assert_eq!(
            config
                .environments
                .get("production")
                .and_then(|environment| environment.compose_file.as_ref()),
            Some(&PathBuf::from("docker/production/compose.yml"))
        );

        let (environment, environment_config) = config
            .resolve_environment_config(None)
            .unwrap()
            .expect("environment should resolve");
        assert_eq!(environment, "production");
        assert_eq!(
            environment_config.compose_file,
            Some(PathBuf::from("docker/production/compose.yml"))
        );
        assert_eq!(
            environment_config.env_file,
            Some(PathBuf::from("docker/production/.env"))
        );
    }

    #[test]
    fn test_environment_override_uses_conventional_compose_path() {
        let yaml = r#"
name: environment-app
app:
  type: static
deploy:
  target: cloud
"#;

        let config = StackerConfig::from_str(yaml).unwrap();
        let (environment, environment_config) = config
            .resolve_environment_config(Some("staging"))
            .unwrap()
            .expect("environment should resolve");

        assert_eq!(environment, "staging");
        assert_eq!(
            environment_config.compose_file,
            Some(PathBuf::from("docker/staging/compose.yml"))
        );
    }

    #[test]
    fn test_monitors_alias_for_monitoring() {
        let yaml = r#"
name: monitors-alias-test
monitors:
  status_panel: true
  healthcheck:
    endpoint: /healthz
    interval: 10s
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        assert!(config.monitoring.status_panel);
        assert!(config.monitoring.healthcheck.is_some());
        let hc = config.monitoring.healthcheck.unwrap();
        assert_eq!(hc.endpoint, "/healthz");
        assert_eq!(hc.interval, "10s");
    }

    #[test]
    fn test_parse_env_var_interpolation() {
        env::set_var("STACKER_TEST_DB_PASS", "secret123");
        let yaml = r#"
name: env-test
app:
  type: static
env:
  DB_PASSWORD: ${STACKER_TEST_DB_PASS}
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.env.get("DB_PASSWORD").unwrap(), "secret123");
        env::remove_var("STACKER_TEST_DB_PASS");
    }

    #[test]
    fn test_parse_env_var_missing_returns_error() {
        // Ensure the var definitely doesn't exist
        env::remove_var("STACKER_TEST_NONEXISTENT_VAR_12345");
        let yaml = r#"
name: env-test
env:
  KEY: ${STACKER_TEST_NONEXISTENT_VAR_12345}
"#;
        let result = StackerConfig::from_str(yaml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("STACKER_TEST_NONEXISTENT_VAR_12345"),
            "Expected var name in error: {msg}"
        );
    }

    #[test]
    fn test_from_str_ignores_env_placeholders_in_comments() {
        let yaml = r#"
name: comment-test
app:
  type: static
# DATABASE_URL: postgres://user:${STACKER_TEST_NONEXISTENT_VAR_54321}@db:5432/app
"#;

        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.name, "comment-test");
        assert_eq!(config.app.app_type, AppType::Static);
    }

    #[test]
    fn test_from_file_resolves_env_from_env_file() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env"), "DOCKER_IMAGE=node:14-alpine\n").unwrap();

        let yaml = r#"
name: env-file-test
env_file: .env
app:
    type: custom
    path: .
    image: ${DOCKER_IMAGE}
deploy:
    target: local
"#;
        let config_path = dir.path().join("stacker.yml");
        fs::write(&config_path, yaml).unwrap();

        let config = StackerConfig::from_file(&config_path).unwrap();
        assert_eq!(config.app.image.as_deref(), Some("node:14-alpine"));
    }

    // Regression tests for GH #239: `stacker deploy --target cloud` failed
    // on a missing `${EXISTING_SERVER_HOST}` even though `deploy.server` is
    // never used for a cloud deploy — env-var resolution walked the whole
    // file unconditionally, including the inactive dual-target section.
    fn dual_target_yaml() -> &'static str {
        r#"
name: dual-target-app
app:
    type: custom
    path: .
    image: myorg/myapp:latest
deploy:
    target: server
    server:
        host: ${EXISTING_SERVER_HOST}
        user: ${EXISTING_SERVER_USER}
        ssh_key: /tmp/id_ed25519
    cloud:
        provider: hetzner
        region: fsn1
        size: cpx22
        public_ports: ["3579"]
"#
    }

    #[test]
    fn test_from_file_for_target_cloud_skips_missing_server_vars() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("stacker.yml");
        fs::write(&config_path, dual_target_yaml()).unwrap();

        // No EXISTING_SERVER_HOST/USER anywhere (env or .env) — must not
        // fail, since --target cloud never touches deploy.server.
        let config = StackerConfig::from_file_for_target(&config_path, Some("cloud")).unwrap();
        let resolved = config.with_resolved_deploy_target(Some("cloud")).unwrap();
        assert_eq!(resolved.deploy.target, DeployTarget::Cloud);
        assert!(resolved.deploy.cloud.is_some());
    }

    #[test]
    fn test_from_file_for_target_server_skips_cloud_section() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("stacker.yml");
        // Cloud section left var-free here on purpose — this test only
        // asserts the server branch resolves independent of cloud content.
        fs::write(
            &config_path,
            r#"
name: dual-target-app
app:
    type: custom
    path: .
    image: myorg/myapp:latest
deploy:
    target: server
    server:
        host: 203.0.113.5
        user: deployer
        ssh_key: /tmp/id_ed25519
    cloud:
        provider: hetzner
        region: ${UNSET_REGION_VAR}
"#,
        )
        .unwrap();

        let config = StackerConfig::from_file_for_target(&config_path, Some("server")).unwrap();
        assert_eq!(
            config.deploy.server.as_ref().map(|s| s.host.as_str()),
            Some("203.0.113.5")
        );
    }

    #[test]
    fn test_from_file_for_target_falls_back_to_literal_deploy_target_in_file() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("stacker.yml");
        fs::write(&config_path, dual_target_yaml()).unwrap();

        // No override passed — `deploy.target: server` in the file is a
        // plain literal, so it should still be trusted as the effective
        // target and fail exactly like before (this isn't a behavior
        // change for callers that already relied on the file's own target).
        let result = StackerConfig::from_file_for_target(&config_path, None);
        assert!(
            result.is_err(),
            "server section is active, so its missing vars must still error"
        );
    }

    #[test]
    fn test_from_file_for_target_unresolvable_target_resolves_both_sections_as_before() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("stacker.yml");
        fs::write(&config_path, dual_target_yaml()).unwrap();

        // Override doesn't match a known target keyword — falls back to
        // resolving everything, matching the pre-fix strict behavior.
        let result = StackerConfig::from_file_for_target(&config_path, Some("bogus"));
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validate_respects_target_override() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("stacker.yml");
        fs::write(&config_path, dual_target_yaml()).unwrap();

        let path_str = config_path.to_string_lossy().to_string();
        assert!(
            crate::console::commands::cli::config::run_validate(&path_str, Some("cloud")).is_ok(),
            "config validate --target cloud must not fail on unset server vars"
        );
        assert!(
            crate::console::commands::cli::config::run_validate(&path_str, None).is_err(),
            "without an override, the file's own deploy.target: server is still active"
        );
    }

    #[test]
    fn test_parse_invalid_app_type_returns_error() {
        let yaml = r#"
name: bad-type
app:
  type: cobol
"#;
        let result = StackerConfig::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_name_returns_error() {
        let yaml = r#"
app:
  type: static
"#;
        // name is a required field — serde fails deserialization if missing
        let result = StackerConfig::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_services_array() {
        let yaml = r#"
name: svc-test
services:
  - name: postgres
    image: postgres:16
    ports: ["5432:5432"]
  - name: redis
    image: redis:7-alpine
  - name: minio
    image: minio/minio
    ports: ["9000:9000", "9001:9001"]
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.services.len(), 3);
        assert_eq!(config.services[0].name, "postgres");
        assert_eq!(config.services[0].image, "postgres:16");
        assert_eq!(config.services[0].ports, vec!["5432:5432"]);
        assert_eq!(config.services[2].name, "minio");
        assert_eq!(config.services[2].ports.len(), 2);
    }

    #[test]
    fn test_parse_services_map() {
        let yaml = r#"
name: svc-map-test
services:
    web:
        name: web
        image: nginx:alpine
        ports: ["8080:80"]
    redis:
        name: redis
        image: redis:7-alpine
"#;

        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.services.len(), 2);
        assert!(config
            .services
            .iter()
            .any(|s| s.name == "web" && s.image == "nginx:alpine"));
        assert!(config
            .services
            .iter()
            .any(|s| s.name == "redis" && s.image == "redis:7-alpine"));
    }

    #[test]
    fn test_parse_services_map_infers_name_from_key() {
        let yaml = r#"
name: svc-map-key-test
services:
    web:
        image: nginx:alpine
        ports: ["8080:80"]
"#;

        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.services.len(), 1);
        assert_eq!(config.services[0].name, "web");
        assert_eq!(config.services[0].image, "nginx:alpine");
    }

    #[test]
    fn test_parse_proxy_domains() {
        let yaml = r#"
name: proxy-test
proxy:
  type: nginx
  domains:
    - domain: app.example.com
      ssl: auto
      upstream: app:3000
    - domain: api.example.com
      ssl: off
      upstream: app:8080
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.proxy.proxy_type, ProxyType::Nginx);
        assert_eq!(config.proxy.domains.len(), 2);
        assert_eq!(config.proxy.domains[0].ssl, SslMode::Auto);
        assert_eq!(config.proxy.domains[0].upstream, "app:3000");
        assert_eq!(config.proxy.domains[1].ssl, SslMode::Off);
    }

    #[test]
    fn test_parse_pipes_block() {
        let yaml = r#"
name: pipes-test
pipes:
  - name: apprise-to-ntfy
    source: app
    target: ntfy
    source_endpoint: "GET /status"
    target_endpoint: "POST /pipetest"
    source_fields: [message]
    target_fields: [message]
    trigger: manual
    retry: 5
    retry_backoff_ms: 500
    on_failure: oncall-notify
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.pipes.len(), 1);
        let p = &config.pipes[0];
        assert_eq!(p.name, "apprise-to-ntfy");
        assert_eq!(p.source_endpoint, "GET /status");
        assert_eq!(p.trigger, "manual");
        // The declared retry/handler flow into the typed PipeConfig …
        let cfg = p.to_pipe_config();
        let retry = cfg.retry.as_ref().expect("retry declared");
        assert_eq!(retry.max_retries, 5);
        assert_eq!(retry.backoff_base_ms, 500);
        assert_eq!(retry.backoff_max_ms, 30_000); // default filled in
        assert_eq!(
            cfg.on_failure,
            Some(crate::models::pipe_config::HandlerRef::Pipe(
                "oncall-notify".into()
            ))
        );
    }

    #[test]
    fn test_pipes_absent_defaults_empty_and_trigger_default() {
        // No pipes: block → empty (back-compat). Minimal pipe → webhook trigger,
        // empty PipeConfig (no retry/handlers declared).
        let config = StackerConfig::from_str("name: no-pipes\n").unwrap();
        assert!(config.pipes.is_empty());

        let yaml = r#"
name: t
pipes:
  - name: p
    source: a
    target: b
    source_endpoint: "GET /x"
    target_endpoint: "POST /y"
"#;
        let c = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(c.pipes[0].trigger, "webhook");
        assert_eq!(
            c.pipes[0].to_pipe_config(),
            crate::models::pipe_config::PipeConfig::default()
        );
    }

    #[test]
    fn test_parse_ai_section_with_ollama() {
        let yaml = r#"
name: ai-test
ai:
  enabled: true
  provider: ollama
  model: llama3
  endpoint: http://localhost:11434
  tasks: [dockerfile, compose]
"#;
        let config = StackerConfig::from_str(yaml).unwrap();
        assert!(config.ai.enabled);
        assert_eq!(config.ai.provider, AiProviderType::Ollama);
        assert_eq!(config.ai.model, Some("llama3".to_string()));
        assert_eq!(
            config.ai.endpoint,
            Some("http://localhost:11434".to_string())
        );
        assert_eq!(config.ai.tasks, vec!["dockerfile", "compose"]);
    }

    #[test]
    fn test_default_deploy_target_is_local() {
        let yaml = "name: minimal\n";
        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.deploy.target, DeployTarget::Local);
    }

    #[test]
    fn test_default_proxy_type_is_none() {
        let yaml = "name: minimal\n";
        let config = StackerConfig::from_str(yaml).unwrap();
        assert_eq!(config.proxy.proxy_type, ProxyType::None);
    }

    #[test]
    fn test_config_file_not_found() {
        let result = StackerConfig::from_file(Path::new("/nonexistent/stacker.yml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CliError::ConfigNotFound { .. }),
            "Expected ConfigNotFound, got: {err:?}"
        );
    }

    #[test]
    fn test_config_invalid_yaml_syntax() {
        let result = StackerConfig::from_str("{{invalid: yaml: :::");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CliError::ConfigParseFailed { .. }),
            "Expected ConfigParseFailed, got: {err:?}"
        );
    }

    #[test]
    fn test_config_invalid_path_reports_field_name() {
        let yaml = r#"
name: bad-path
app:
  type: custom
  path: {}
"#;
        let err = StackerConfig::from_str(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("app.path"), "unexpected message: {msg}");
        assert!(
            msg.contains("quoted path string"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn test_validate_semantics_cloud_without_provider() {
        let config = ConfigBuilder::new()
            .name("test")
            .deploy_target(DeployTarget::Cloud)
            .build()
            .unwrap();

        let issues = config.validate_semantics();
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            !errors.is_empty(),
            "Expected validation error for missing cloud provider"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.field.as_deref() == Some("deploy.cloud.provider")),
            "Expected field reference to deploy.cloud.provider"
        );
    }

    #[test]
    fn test_validate_semantics_server_without_host() {
        let config = ConfigBuilder::new()
            .name("test")
            .deploy_target(DeployTarget::Server)
            .build()
            .unwrap();

        let issues = config.validate_semantics();
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            !errors.is_empty(),
            "Expected validation error for missing server host"
        );
        assert!(
            errors.iter().any(|e| e.message.contains("host")),
            "Expected 'host' mentioned in error"
        );
    }

    // --- E007: port mappings must be real ports -----------------------
    //
    // A GitLab deploy provisioned a server, shipped the compose, and only then
    // died on the host with `invalid containerPort: 133342`. `app.ports` went
    // into the generated compose verbatim with no range check; a bare
    // out-of-range entry is read by Docker as a container port.

    #[test]
    fn validate_port_mapping_accepts_every_compose_shape() {
        for good in [
            "80",
            "8080:80",
            "127.0.0.1:5432:5432",
            "[::1]:8080:80",
            "8000-8010:8000-8010",
            "2222:22/tcp",
            "53:53/udp",
            "65535:65535",
        ] {
            assert!(
                validate_port_mapping(good).is_ok(),
                "{good} should be valid: {:?}",
                validate_port_mapping(good)
            );
        }
    }

    #[test]
    fn validate_port_mapping_rejects_out_of_range_and_malformed() {
        for bad in [
            "133342",
            "8082:133342",
            "0",
            "65536",
            "127.0.0.1:0:80",
            "abc:80",
            "",
            "80/http",
            "1:2:3:4",
        ] {
            assert!(
                validate_port_mapping(bad).is_err(),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn e007_flags_out_of_range_app_port() {
        let config = ConfigBuilder::new()
            .name("gitlab")
            .app_type(AppType::Custom)
            .app_image("gitlab/gitlab-ce:18.10.1-ce.0")
            .app_ports(vec!["133342".to_string()])
            .build()
            .unwrap();

        let issues = config.validate_semantics();
        let e007: Vec<_> = issues
            .iter()
            .filter(|i| i.code == "E007" && i.severity == Severity::Error)
            .collect();

        assert_eq!(e007.len(), 1, "expected one E007, got: {issues:?}");
        assert_eq!(e007[0].field.as_deref(), Some("app.ports"));
        assert!(e007[0].message.contains("133342"));
    }

    #[test]
    fn e007_names_the_offending_service() {
        let svc = ServiceDefinition {
            name: "db".to_string(),
            image: "postgres:16".to_string(),
            ports: vec!["5432:5432".to_string(), "99999:5432".to_string()],
            environment: HashMap::new(),
            volumes: vec![],
            depends_on: vec![],
            command: None,
            healthcheck: None,
        };

        let config = ConfigBuilder::new()
            .name("stack")
            .add_service(svc)
            .build()
            .unwrap();

        let e007: Vec<_> = config
            .validate_semantics()
            .into_iter()
            .filter(|i| i.code == "E007")
            .collect();

        assert_eq!(e007.len(), 1, "only the bad mapping should be flagged");
        assert_eq!(e007[0].field.as_deref(), Some("services.db.ports"));
    }

    #[test]
    fn e007_stays_quiet_on_valid_ports() {
        let config = ConfigBuilder::new()
            .name("gitlab")
            .app_type(AppType::Custom)
            .app_image("gitlab/gitlab-ce:18.10.1-ce.0")
            .app_ports(vec!["8082:80".to_string(), "2222:22".to_string()])
            .build()
            .unwrap();

        assert!(
            !config
                .validate_semantics()
                .iter()
                .any(|i| i.code == "E007"),
            "valid ports must not raise E007"
        );
    }

    #[test]
    fn test_validate_semantics_port_conflict() {
        let config = StackerConfig::from_str(
            r#"
name: port-conflict
services:
  - name: web1
    image: nginx
    ports: ["8080:80"]
  - name: web2
    image: httpd
    ports: ["8080:80"]
"#,
        )
        .unwrap();

        let issues = config.validate_semantics();
        let warnings: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect();
        assert!(!warnings.is_empty(), "Expected warning about port conflict");
        assert!(
            warnings.iter().any(|w| w.message.contains("8080")),
            "Expected port 8080 in warning"
        );
    }

    #[test]
    fn test_validate_semantics_no_image_no_dockerfile_custom() {
        let config = ConfigBuilder::new()
            .name("test")
            .app_type(AppType::Custom)
            .build()
            .unwrap();

        let issues = config.validate_semantics();
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            !errors.is_empty(),
            "Expected error for custom type without image or dockerfile"
        );
    }

    #[test]
    fn test_validate_semantics_happy_path() {
        let config = ConfigBuilder::new()
            .name("valid-app")
            .app_type(AppType::Static)
            .build()
            .unwrap();

        let issues = config.validate_semantics();
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn w001_does_not_false_positive_on_distinct_loopback_ports() {
        // Regression: `ip:host:container` bindings must key on the host *port*,
        // not the IP. Two loopback services on different ports do NOT conflict.
        let config = StackerConfig::from_str(
            r#"
name: demo
app:
  type: custom
  image: myapp:latest
services:
  - name: postgres
    image: postgres:16-alpine
    ports:
      - "127.0.0.1:5432:5432"
  - name: redis
    image: redis:7-alpine
    ports:
      - "127.0.0.1:6379:6379"
"#,
        )
        .unwrap();

        assert!(
            !config
                .validate_semantics()
                .iter()
                .any(|issue| issue.code == "W001"),
            "distinct loopback host ports must not be reported as a conflict"
        );
    }

    #[test]
    fn w001_detects_real_host_port_conflict_across_binding_forms() {
        // A 2-part `80:80` and a 3-part `127.0.0.1:80:80` both publish host
        // port 80 → real conflict, previously missed by the IP-first extractor.
        let config = StackerConfig::from_str(
            r#"
name: demo
app:
  type: custom
  image: myapp:latest
services:
  - name: web
    image: nginx:alpine
    ports:
      - "80:80"
  - name: legacy
    image: httpd:alpine
    ports:
      - "127.0.0.1:80:80"
"#,
        )
        .unwrap();

        let w001: Vec<_> = config
            .validate_semantics()
            .into_iter()
            .filter(|issue| issue.code == "W001")
            .collect();
        assert_eq!(
            w001.len(),
            1,
            "expected one W001 for the port-80 clash: {w001:?}"
        );
        assert!(w001[0].message.contains("80"));
        assert!(w001[0].message.contains("web") && w001[0].message.contains("legacy"));
    }

    #[test]
    fn proxy_block_warns_when_user_service_publishes_an_ingress_port() {
        // A `proxy:` block deploys a platform-managed proxy owning 80/443/81.
        // A user's own reverse-proxy service on 80/443 collides → one W003.
        let config = StackerConfig::from_str(
            r#"
name: demo
app:
  type: custom
  image: myapp:latest
services:
  - name: my-own-traefik
    image: traefik:v2.10
    ports:
      - "80:80"
      - "443:443"
proxy:
  type: nginx-proxy-manager
"#,
        )
        .unwrap();

        let w003: Vec<_> = config
            .validate_semantics()
            .into_iter()
            .filter(|issue| issue.code == "W003")
            .collect();
        assert_eq!(w003.len(), 1, "expected exactly one W003, got: {w003:?}");
        assert_eq!(w003[0].severity, Severity::Warning);
        assert!(w003[0].message.contains("my-own-traefik"));
        // Both conflicting ports are reported in the single per-service warning.
        assert!(w003[0].message.contains("80"));
        assert!(w003[0].message.contains("443"));
    }

    #[test]
    fn proxy_block_does_not_warn_for_nonconflicting_ports() {
        // The app on :8080 (with `127.0.0.1:` host-scoped DB elsewhere) does not
        // overlap the proxy's ingress ports → no W003.
        let config = StackerConfig::from_str(
            r#"
name: demo
app:
  type: custom
  image: myapp:latest
  ports:
    - "8080:8080"
services:
  - name: db
    image: postgres:16-alpine
    ports:
      - "127.0.0.1:5432:5432"
proxy:
  type: nginx-proxy-manager
"#,
        )
        .unwrap();

        assert!(
            !config
                .validate_semantics()
                .iter()
                .any(|issue| issue.code == "W003"),
            "no ingress overlap should produce no W003"
        );
    }

    #[test]
    fn test_validate_semantics_multi_target_requires_default_for_multiple_profiles() {
        let config = StackerConfig::from_str(
            r#"
name: multi-target-app
app:
  type: static
deploy:
  targets:
    local:
      compose_file: docker/local/compose.yml
    prod:
      server:
        host: 10.0.0.8
        user: deploy
        ssh_key: ~/.ssh/id_ed25519
"#,
        )
        .unwrap();

        let issues = config.validate_semantics();
        assert!(issues.iter().any(|issue| issue.code == "E004"));
    }

    #[test]
    fn test_validate_semantics_multi_target_rejects_ambiguous_profile() {
        let config = StackerConfig::from_str(
            r#"
name: multi-target-app
app:
  type: static
deploy:
  default_target: hybrid
  targets:
    hybrid:
      cloud:
        provider: aws
      server:
        host: 10.0.0.8
        user: deploy
        ssh_key: ~/.ssh/id_ed25519
"#,
        )
        .unwrap();

        let issues = config.validate_semantics();
        assert!(issues.iter().any(|issue| issue.code == "E006"));
    }

    #[test]
    fn test_validate_semantics_remote_cloud_defaults_stack_code_without_project_identity() {
        let config = ConfigBuilder::new()
            .name("remote-app")
            .deploy_target(DeployTarget::Cloud)
            .cloud(CloudConfig {
                provider: CloudProvider::Hetzner,
                orchestrator: CloudOrchestrator::Remote,
                region: Some("nbg1".to_string()),
                size: Some("cx23".to_string()),
                install_image: None,
                remote_payload_file: None,
                ssh_key: None,
                key: None,
                server: None,
                public_ports: Vec::new(),
            })
            .build()
            .unwrap();

        let issues = config.validate_semantics();
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        let infos: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Info)
            .collect();
        assert!(
            errors.is_empty(),
            "Expected no blocking errors, got: {errors:?}"
        );
        assert!(
            infos
                .iter()
                .any(|e| e.field.as_deref() == Some("project.identity")),
            "Expected project.identity informational hint"
        );
    }

    #[test]
    fn test_validate_semantics_cloud_public_ports_accepts_bare_number() {
        let config = ConfigBuilder::new()
            .name("ports-app")
            .deploy_target(DeployTarget::Cloud)
            .cloud(CloudConfig {
                provider: CloudProvider::Hetzner,
                orchestrator: CloudOrchestrator::Local,
                region: Some("fsn1".to_string()),
                size: Some("cpx21".to_string()),
                install_image: None,
                remote_payload_file: None,
                ssh_key: None,
                key: None,
                server: None,
                public_ports: vec!["8000".to_string(), "443/tcp".to_string()],
            })
            .build()
            .unwrap();

        let issues = config.validate_semantics();
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error && i.code == "E005")
            .collect();
        assert!(
            errors.is_empty(),
            "Bare port numbers and port/proto specs must be valid, got: {errors:?}"
        );
    }

    #[test]
    fn test_validate_semantics_cloud_public_ports_rejects_invalid() {
        let config = ConfigBuilder::new()
            .name("ports-app")
            .deploy_target(DeployTarget::Cloud)
            .cloud(CloudConfig {
                provider: CloudProvider::Hetzner,
                orchestrator: CloudOrchestrator::Local,
                region: Some("fsn1".to_string()),
                size: Some("cpx21".to_string()),
                install_image: None,
                remote_payload_file: None,
                ssh_key: None,
                key: None,
                server: None,
                public_ports: vec!["80/icmp".to_string(), "not-a-port".to_string()],
            })
            .build()
            .unwrap();

        let issues = config.validate_semantics();
        let e005: Vec<_> = issues.iter().filter(|i| i.code == "E005").collect();
        assert_eq!(
            e005.len(),
            2,
            "Expected two E005 issues for invalid public_ports, got: {e005:?}"
        );
    }

    // ━━━ ConfigBuilder tests ━━━

    #[test]
    fn test_config_builder_minimal() {
        let config = ConfigBuilder::new().name("test").build().unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.app.app_type, AppType::Static);
        assert_eq!(config.app.path, PathBuf::from("."));
        assert_eq!(config.deploy.target, DeployTarget::Local);
        assert_eq!(config.project.identity, None);
    }

    #[test]
    fn test_config_builder_project_identity() {
        let config = ConfigBuilder::new()
            .name("test")
            .project_identity("registered-stack-code")
            .build()
            .unwrap();
        assert_eq!(
            config.project.identity.as_deref(),
            Some("registered-stack-code")
        );
    }

    #[test]
    fn test_config_builder_fluent_chain() {
        let config = ConfigBuilder::new()
            .name("my-app")
            .version("1.0")
            .organization("acme")
            .app_type(AppType::Node)
            .app_path("./src")
            .add_service(ServiceDefinition {
                name: "postgres".to_string(),
                image: "postgres:16".to_string(),
                ports: vec!["5432:5432".to_string()],
                environment: HashMap::new(),
                volumes: vec![],
                depends_on: vec![],
                command: None,
                healthcheck: None,
            })
            .deploy_target(DeployTarget::Cloud)
            .cloud(CloudConfig {
                provider: CloudProvider::Hetzner,
                orchestrator: CloudOrchestrator::Local,
                region: Some("fsn1".to_string()),
                size: Some("cpx21".to_string()),
                install_image: None,
                remote_payload_file: None,
                ssh_key: None,
                key: None,
                server: None,
                public_ports: Vec::new(),
            })
            .build()
            .unwrap();

        assert_eq!(config.name, "my-app");
        assert_eq!(config.version, Some("1.0".to_string()));
        assert_eq!(config.organization, Some("acme".to_string()));
        assert_eq!(config.app.app_type, AppType::Node);
        assert_eq!(config.app.path, PathBuf::from("./src"));
        assert_eq!(config.services.len(), 1);
        assert_eq!(config.deploy.target, DeployTarget::Cloud);
        assert!(config.deploy.cloud.is_some());
    }

    #[test]
    fn test_config_builder_missing_name_returns_error() {
        let result = ConfigBuilder::new().app_type(AppType::Static).build();
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("name"), "Expected 'name' in error: {msg}");
    }

    #[test]
    fn test_config_builder_default_app_type_is_static() {
        let config = ConfigBuilder::new().name("x").build().unwrap();
        assert_eq!(config.app.app_type, AppType::Static);
    }

    #[test]
    fn test_config_builder_to_yaml_roundtrip() {
        let original = ConfigBuilder::new()
            .name("roundtrip")
            .app_type(AppType::Python)
            .app_path("./app")
            .env("PORT", "8000")
            .build()
            .unwrap();

        let yaml = serde_yaml::to_string(&original).unwrap();
        let parsed = StackerConfig::from_str(&yaml).unwrap();

        assert_eq!(original.name, parsed.name);
        assert_eq!(original.app.app_type, parsed.app.app_type);
        assert_eq!(original.app.path, parsed.app.path);
        assert_eq!(original.env.get("PORT"), parsed.env.get("PORT"));
    }

    #[test]
    fn test_config_builder_multiple_services() {
        let config = ConfigBuilder::new()
            .name("multi")
            .add_service(ServiceDefinition {
                name: "pg".to_string(),
                image: "postgres:16".to_string(),
                ports: vec![],
                environment: HashMap::new(),
                volumes: vec![],
                depends_on: vec![],
                command: None,
                healthcheck: None,
            })
            .add_service(ServiceDefinition {
                name: "redis".to_string(),
                image: "redis:7".to_string(),
                ports: vec![],
                environment: HashMap::new(),
                volumes: vec![],
                depends_on: vec![],
                command: None,
                healthcheck: None,
            })
            .add_service(ServiceDefinition {
                name: "minio".to_string(),
                image: "minio/minio".to_string(),
                ports: vec![],
                environment: HashMap::new(),
                volumes: vec![],
                depends_on: vec![],
                command: None,
                healthcheck: None,
            })
            .build()
            .unwrap();

        assert_eq!(config.services.len(), 3);
    }

    // ━━━ Enum tests ━━━

    #[test]
    fn test_app_type_display() {
        assert_eq!(format!("{}", AppType::Static), "static");
        assert_eq!(format!("{}", AppType::Node), "node");
        assert_eq!(format!("{}", AppType::Python), "python");
        assert_eq!(format!("{}", AppType::Rust), "rust");
        assert_eq!(format!("{}", AppType::Go), "go");
        assert_eq!(format!("{}", AppType::Php), "php");
        assert_eq!(format!("{}", AppType::Custom), "custom");
    }

    #[test]
    fn test_app_type_serde_roundtrip() {
        let json = serde_json::to_string(&AppType::Node).unwrap();
        assert_eq!(json, "\"node\"");
        let parsed: AppType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, AppType::Node);
    }

    #[test]
    fn test_app_type_default_is_static() {
        assert_eq!(AppType::default(), AppType::Static);
    }

    #[test]
    fn test_deploy_target_display() {
        assert_eq!(format!("{}", DeployTarget::Local), "local");
        assert_eq!(format!("{}", DeployTarget::Cloud), "cloud");
        assert_eq!(format!("{}", DeployTarget::Server), "server");
    }

    #[test]
    fn test_deploy_target_default_is_local() {
        assert_eq!(DeployTarget::default(), DeployTarget::Local);
    }

    #[test]
    fn test_proxy_type_display() {
        assert_eq!(format!("{}", ProxyType::Nginx), "nginx");
        assert_eq!(
            format!("{}", ProxyType::NginxProxyManager),
            "nginx-proxy-manager"
        );
        assert_eq!(format!("{}", ProxyType::Traefik), "traefik");
        assert_eq!(format!("{}", ProxyType::None), "none");
    }

    #[test]
    fn test_proxy_type_default_is_none() {
        assert_eq!(ProxyType::default(), ProxyType::None);
    }
}
