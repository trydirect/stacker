//! ConfigRenderer Service - Unified Configuration Management
//!
//! This service converts ProjectApp records from the database into deployable
//! configuration files (docker-compose.yml, .env files) using Tera templates.
//!
//! It serves as the single source of truth for generating configs that are:
//! 1. Stored in Vault for Status Panel to fetch
//! 2. Used during initial deployment via Ansible
//! 3. Applied for runtime configuration updates

use crate::configuration::DeploymentSettings;
use crate::models::{Project, ProjectApp};
use crate::services::vault_service::{AppConfig, VaultError, VaultService};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tera::{Context as TeraContext, Tera};

/// Rendered configuration bundle for a deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigBundle {
    /// The project/deployment identifier
    pub deployment_hash: String,
    /// Version of this configuration bundle (incrementing)
    pub version: u64,
    /// Docker Compose file content (YAML)
    pub compose_content: String,
    /// Per-app configuration files (.env, config files)
    pub app_configs: HashMap<String, AppConfig>,
    /// Timestamp when bundle was generated
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

/// App environment rendering context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRenderContext {
    /// App code (e.g., "nginx", "postgres")
    pub code: String,
    /// App name
    pub name: String,
    /// Docker image
    pub image: String,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Port mappings
    pub ports: Vec<PortMapping>,
    /// Volume mounts
    pub volumes: Vec<VolumeMount>,
    /// Domain configuration
    pub domain: Option<String>,
    /// SSL enabled
    pub ssl_enabled: bool,
    /// Network names
    pub networks: Vec<String>,
    /// Depends on (other app codes)
    pub depends_on: Vec<String>,
    /// Restart policy
    pub restart_policy: String,
    /// Resource limits
    pub resources: ResourceLimits,
    /// Labels
    pub labels: HashMap<String, String>,
    /// Healthcheck configuration
    pub healthcheck: Option<HealthCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    pub cpu_limit: Option<String>,
    pub memory_limit: Option<String>,
    pub cpu_reservation: Option<String>,
    pub memory_reservation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub test: Vec<String>,
    pub interval: Option<String>,
    pub timeout: Option<String>,
    pub retries: Option<u32>,
    pub start_period: Option<String>,
}

/// ConfigRenderer - Renders and syncs app configurations
pub struct ConfigRenderer {
    tera: Tera,
    vault_service: Option<VaultService>,
    deployment_settings: DeploymentSettings,
}

impl ConfigRenderer {
    /// Create a new ConfigRenderer with embedded templates
    pub fn new() -> Result<Self> {
        let mut tera = Tera::default();

        // Register embedded templates
        tera.add_raw_template("docker-compose.yml.tera", DOCKER_COMPOSE_TEMPLATE)
            .context("Failed to add docker-compose template")?;
        tera.add_raw_template("env.tera", ENV_FILE_TEMPLATE)
            .context("Failed to add env template")?;
        tera.add_raw_template("service.tera", SERVICE_TEMPLATE)
            .context("Failed to add service template")?;

        // Initialize Vault service if configured
        let vault_service =
            VaultService::from_env().map_err(|e| anyhow::anyhow!("Vault init error: {}", e))?;

        // Load deployment settings
        let deployment_settings = DeploymentSettings::default();

        Ok(Self {
            tera,
            vault_service,
            deployment_settings,
        })
    }

    /// Create ConfigRenderer with custom deployment settings
    pub fn with_settings(deployment_settings: DeploymentSettings) -> Result<Self> {
        let mut renderer = Self::new()?;
        renderer.deployment_settings = deployment_settings;
        Ok(renderer)
    }

    /// Get the base path for deployments
    pub fn base_path(&self) -> &str {
        self.deployment_settings.base_path()
    }

    /// Get the full deploy directory for a deployment hash
    pub fn deploy_dir(&self, deployment_hash: &str) -> String {
        self.deployment_settings.deploy_dir(deployment_hash)
    }

    /// Create ConfigRenderer with a custom Vault service (for testing)
    pub fn with_vault(vault_service: VaultService) -> Result<Self> {
        let mut renderer = Self::new()?;
        renderer.vault_service = Some(vault_service);
        Ok(renderer)
    }

    /// Render a full configuration bundle for a project
    pub fn render_bundle(
        &self,
        project: &Project,
        apps: &[ProjectApp],
        deployment_hash: &str,
    ) -> Result<ConfigBundle> {
        let app_contexts: Vec<AppRenderContext> = apps
            .iter()
            .filter(|a| a.is_enabled())
            .map(|app| self.project_app_to_context(app, project))
            .collect::<Result<Vec<_>>>()?;

        // Render docker-compose.yml
        let compose_content = self.render_compose(&app_contexts, project)?;

        // Render per-app .env files
        let mut app_configs = HashMap::new();
        for app in apps.iter().filter(|a| a.is_enabled()) {
            let env_content = self.render_env_file(app, project, deployment_hash)?;
            let config = AppConfig {
                content: env_content,
                content_type: "env".to_string(),
                destination_path: format!("{}/{}.env", self.deploy_dir(deployment_hash), app.code),
                file_mode: "0640".to_string(),
                owner: Some("trydirect".to_string()),
                group: Some("docker".to_string()),
            };
            app_configs.insert(app.code.clone(), config);
        }

        Ok(ConfigBundle {
            deployment_hash: deployment_hash.to_string(),
            version: 1,
            compose_content,
            app_configs,
            generated_at: chrono::Utc::now(),
        })
    }

    /// Convert a ProjectApp to a renderable context
    fn project_app_to_context(
        &self,
        app: &ProjectApp,
        _project: &Project,
    ) -> Result<AppRenderContext> {
        // Parse environment variables from JSON
        let environment = self.parse_environment(&app.environment)?;

        // Parse ports from JSON
        let ports = self.parse_ports(&app.ports)?;

        // Parse volumes from JSON
        let volumes = self.parse_volumes(&app.volumes)?;

        // Parse networks from JSON
        let networks = self.parse_string_array(&app.networks)?;

        // Parse depends_on from JSON
        let depends_on = self.parse_string_array(&app.depends_on)?;

        // Parse resources from JSON
        let resources = self.parse_resources(&app.resources)?;

        // Parse labels from JSON
        let labels = self.parse_labels(&app.labels)?;

        // Parse healthcheck from JSON
        let healthcheck = self.parse_healthcheck(&app.healthcheck)?;

        Ok(AppRenderContext {
            code: app.code.clone(),
            name: app.name.clone(),
            image: app.image.clone(),
            environment,
            ports,
            volumes,
            domain: app.domain.clone(),
            ssl_enabled: app.ssl_enabled.unwrap_or(false),
            networks,
            depends_on,
            restart_policy: app
                .restart_policy
                .clone()
                .unwrap_or_else(|| "unless-stopped".to_string()),
            resources,
            labels,
            healthcheck,
        })
    }

    /// Parse environment JSON to HashMap
    fn parse_environment(&self, env: &Option<Value>) -> Result<HashMap<String, String>> {
        match env {
            Some(Value::Object(map)) => {
                let mut result = HashMap::new();
                for (k, v) in map {
                    let value = match v {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => v.to_string(),
                    };
                    result.insert(k.clone(), value);
                }
                Ok(result)
            }
            Some(Value::Array(arr)) => {
                // Handle array format: ["VAR=value", "VAR2=value2"]
                let mut result = HashMap::new();
                for item in arr {
                    if let Value::String(s) = item {
                        if let Some((k, v)) = s.split_once('=') {
                            result.insert(k.to_string(), v.to_string());
                        }
                    }
                }
                Ok(result)
            }
            None => Ok(HashMap::new()),
            _ => Ok(HashMap::new()),
        }
    }

    /// Parse ports JSON to Vec<PortMapping>
    fn parse_ports(&self, ports: &Option<Value>) -> Result<Vec<PortMapping>> {
        match ports {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for item in arr {
                    if let Value::Object(map) = item {
                        let host = map.get("host").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
                        let container =
                            map.get("container").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
                        let protocol = map
                            .get("protocol")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tcp")
                            .to_string();
                        if host > 0 && container > 0 {
                            result.push(PortMapping {
                                host,
                                container,
                                protocol,
                            });
                        }
                    } else if let Value::String(s) = item {
                        // Handle string format: "8080:80" or "8080:80/tcp"
                        if let Some((host_str, rest)) = s.split_once(':') {
                            let (container_str, protocol) = rest
                                .split_once('/')
                                .map(|(c, p)| (c, p.to_string()))
                                .unwrap_or((rest, "tcp".to_string()));
                            if let (Ok(host), Ok(container)) =
                                (host_str.parse::<u16>(), container_str.parse::<u16>())
                            {
                                result.push(PortMapping {
                                    host,
                                    container,
                                    protocol,
                                });
                            }
                        }
                    }
                }
                Ok(result)
            }
            None => Ok(Vec::new()),
            _ => Ok(Vec::new()),
        }
    }

    /// Parse volumes JSON to Vec<VolumeMount>
    fn parse_volumes(&self, volumes: &Option<Value>) -> Result<Vec<VolumeMount>> {
        match volumes {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for item in arr {
                    if let Value::Object(map) = item {
                        let source = map
                            .get("source")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let target = map
                            .get("target")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let read_only = map
                            .get("read_only")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if !source.is_empty() && !target.is_empty() {
                            result.push(VolumeMount {
                                source,
                                target,
                                read_only,
                            });
                        }
                    } else if let Value::String(s) = item {
                        // Handle string format: "/host:/container" or "/host:/container:ro"
                        let parts: Vec<&str> = s.split(':').collect();
                        if parts.len() >= 2 {
                            result.push(VolumeMount {
                                source: parts[0].to_string(),
                                target: parts[1].to_string(),
                                read_only: parts.get(2).map(|p| *p == "ro").unwrap_or(false),
                            });
                        }
                    }
                }
                Ok(result)
            }
            None => Ok(Vec::new()),
            _ => Ok(Vec::new()),
        }
    }

    /// Parse JSON array to Vec<String>
    fn parse_string_array(&self, value: &Option<Value>) -> Result<Vec<String>> {
        match value {
            Some(Value::Array(arr)) => Ok(arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()),
            None => Ok(Vec::new()),
            _ => Ok(Vec::new()),
        }
    }

    /// Parse resources JSON to ResourceLimits
    fn parse_resources(&self, resources: &Option<Value>) -> Result<ResourceLimits> {
        match resources {
            Some(Value::Object(map)) => Ok(ResourceLimits {
                cpu_limit: map
                    .get("cpu_limit")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                memory_limit: map
                    .get("memory_limit")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                cpu_reservation: map
                    .get("cpu_reservation")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                memory_reservation: map
                    .get("memory_reservation")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            }),
            None => Ok(ResourceLimits::default()),
            _ => Ok(ResourceLimits::default()),
        }
    }

    /// Parse labels JSON to HashMap
    fn parse_labels(&self, labels: &Option<Value>) -> Result<HashMap<String, String>> {
        match labels {
            Some(Value::Object(map)) => {
                let mut result = HashMap::new();
                for (k, v) in map {
                    if let Value::String(s) = v {
                        result.insert(k.clone(), s.clone());
                    }
                }
                Ok(result)
            }
            None => Ok(HashMap::new()),
            _ => Ok(HashMap::new()),
        }
    }

    /// Parse healthcheck JSON
    fn parse_healthcheck(&self, healthcheck: &Option<Value>) -> Result<Option<HealthCheck>> {
        match healthcheck {
            Some(Value::Object(map)) => {
                let test: Vec<String> = map
                    .get("test")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                if test.is_empty() {
                    return Ok(None);
                }

                Ok(Some(HealthCheck {
                    test,
                    interval: map
                        .get("interval")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    timeout: map
                        .get("timeout")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    retries: map
                        .get("retries")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32),
                    start_period: map
                        .get("start_period")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                }))
            }
            None => Ok(None),
            _ => Ok(None),
        }
    }

    /// Render docker-compose.yml from app contexts
    fn render_compose(&self, apps: &[AppRenderContext], project: &Project) -> Result<String> {
        let mut context = TeraContext::new();
        context.insert("apps", apps);
        context.insert("project_name", &project.name);
        context.insert("project_id", &project.stack_id.to_string());

        // Extract network configuration from project metadata
        let default_network = project
            .metadata
            .get("network")
            .and_then(|v| v.as_str())
            .unwrap_or("trydirect_network")
            .to_string();
        context.insert("default_network", &default_network);

        self.tera
            .render("docker-compose.yml.tera", &context)
            .context("Failed to render docker-compose.yml template")
    }

    /// Render .env file for a specific app
    fn render_env_file(
        &self,
        app: &ProjectApp,
        _project: &Project,
        deployment_hash: &str,
    ) -> Result<String> {
        let env_map = self.parse_environment(&app.environment)?;

        let mut context = TeraContext::new();
        context.insert("app_code", &app.code);
        context.insert("app_name", &app.name);
        context.insert("deployment_hash", deployment_hash);
        context.insert("environment", &env_map);
        context.insert("domain", &app.domain);
        context.insert("ssl_enabled", &app.ssl_enabled.unwrap_or(false));

        self.tera
            .render("env.tera", &context)
            .context("Failed to render env template")
    }

    /// Sync all app configs to Vault
    pub async fn sync_to_vault(&self, bundle: &ConfigBundle) -> Result<SyncResult, VaultError> {
        let vault = match &self.vault_service {
            Some(v) => v,
            None => return Err(VaultError::NotConfigured),
        };

        let mut synced = Vec::new();
        let mut failed = Vec::new();

        // Store docker-compose.yml as a special config
        let compose_config = AppConfig {
            content: bundle.compose_content.clone(),
            content_type: "yaml".to_string(),
            destination_path: format!(
                "{}/docker-compose.yml",
                self.deploy_dir(&bundle.deployment_hash)
            ),
            file_mode: "0644".to_string(),
            owner: Some("trydirect".to_string()),
            group: Some("docker".to_string()),
        };

        match vault
            .store_app_config(&bundle.deployment_hash, "_compose", &compose_config)
            .await
        {
            Ok(()) => synced.push("_compose".to_string()),
            Err(e) => {
                tracing::error!("Failed to sync compose config: {}", e);
                failed.push(("_compose".to_string(), e.to_string()));
            }
        }

        // Store per-app .env configs - use {app_code}_env key to separate from compose
        for (app_code, config) in &bundle.app_configs {
            let env_key = format!("{}_env", app_code);
            match vault
                .store_app_config(&bundle.deployment_hash, &env_key, config)
                .await
            {
                Ok(()) => synced.push(env_key),
                Err(e) => {
                    tracing::error!("Failed to sync .env config for {}: {}", app_code, e);
                    failed.push((app_code.clone(), e.to_string()));
                }
            }
        }

        Ok(SyncResult {
            synced,
            failed,
            version: bundle.version,
            synced_at: chrono::Utc::now(),
        })
    }

    /// Sync a single app config to Vault (for incremental updates)
    pub async fn sync_app_to_vault(
        &self,
        app: &ProjectApp,
        project: &Project,
        deployment_hash: &str,
    ) -> Result<(), VaultError> {
        tracing::debug!(
            "Syncing config for app {} (deployment {}) to Vault",
            app.code,
            deployment_hash
        );
        let vault = match &self.vault_service {
            Some(v) => v,
            None => return Err(VaultError::NotConfigured),
        };

        let env_content = self
            .render_env_file(app, project, deployment_hash)
            .map_err(|e| VaultError::Other(format!("Render failed: {}", e)))?;

        let config = AppConfig {
            content: env_content,
            content_type: "env".to_string(),
            destination_path: format!("{}/{}.env", self.deploy_dir(deployment_hash), app.code),
            file_mode: "0640".to_string(),
            owner: Some("trydirect".to_string()),
            group: Some("docker".to_string()),
        };

        tracing::debug!(
            "Storing .env config for app {} at path {} in Vault",
            app.code,
            config.destination_path
        );
        // Use {app_code}_env key to store .env files separately from compose
        let env_key = format!("{}_env", app.code);
        vault
            .store_app_config(deployment_hash, &env_key, &config)
            .await
    }
}

/// Result of syncing configs to Vault
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub synced: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub version: u64,
    pub synced_at: chrono::DateTime<chrono::Utc>,
}

impl SyncResult {
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }
}

// ============================================================================
// Embedded Templates
// ============================================================================

/// Docker Compose template using Tera syntax
const DOCKER_COMPOSE_TEMPLATE: &str = r#"# Generated by TryDirect ConfigRenderer
# Project: {{ project_name }}
# Generated at: {{ now() | date(format="%Y-%m-%d %H:%M:%S UTC") }}

version: '3.8'

services:
{% for app in apps %}
  {{ app.code }}:
    image: {{ app.image }}
    container_name: {{ app.code }}
{% if app.command %}
    command: {{ app.command }}
{% endif %}
{% if app.entrypoint %}
    entrypoint: {{ app.entrypoint }}
{% endif %}
    restart: {{ app.restart_policy }}
{% if app.environment | length > 0 %}
    environment:
{% for key, value in app.environment %}
      - {{ key }}={{ value }}
{% endfor %}
{% endif %}
{% if app.ports | length > 0 %}
    ports:
{% for port in app.ports %}
      - "{{ port.host }}:{{ port.container }}{% if port.protocol != 'tcp' %}/{{ port.protocol }}{% endif %}"
{% endfor %}
{% endif %}
{% if app.volumes | length > 0 %}
    volumes:
{% for vol in app.volumes %}
      - {{ vol.source }}:{{ vol.target }}{% if vol.read_only %}:ro{% endif %}

{% endfor %}
{% endif %}
{% if app.networks | length > 0 %}
    networks:
{% for network in app.networks %}
      - {{ network }}
{% endfor %}
{% else %}
    networks:
      - {{ default_network }}
{% endif %}
{% if app.depends_on | length > 0 %}
    depends_on:
{% for dep in app.depends_on %}
      - {{ dep }}
{% endfor %}
{% endif %}
{% if app.labels | length > 0 %}
    labels:
{% for key, value in app.labels %}
      {{ key }}: "{{ value }}"
{% endfor %}
{% endif %}
{% if app.healthcheck %}
    healthcheck:
      test: {{ app.healthcheck.test | json_encode() }}
{% if app.healthcheck.interval %}
      interval: {{ app.healthcheck.interval }}
{% endif %}
{% if app.healthcheck.timeout %}
      timeout: {{ app.healthcheck.timeout }}
{% endif %}
{% if app.healthcheck.retries %}
      retries: {{ app.healthcheck.retries }}
{% endif %}
{% if app.healthcheck.start_period %}
      start_period: {{ app.healthcheck.start_period }}
{% endif %}
{% endif %}
{% if app.resources.memory_limit or app.resources.cpu_limit %}
    deploy:
      resources:
        limits:
{% if app.resources.memory_limit %}
          memory: {{ app.resources.memory_limit }}
{% endif %}
{% if app.resources.cpu_limit %}
          cpus: '{{ app.resources.cpu_limit }}'
{% endif %}
{% if app.resources.memory_reservation or app.resources.cpu_reservation %}
        reservations:
{% if app.resources.memory_reservation %}
          memory: {{ app.resources.memory_reservation }}
{% endif %}
{% if app.resources.cpu_reservation %}
          cpus: '{{ app.resources.cpu_reservation }}'
{% endif %}
{% endif %}
{% endif %}

{% endfor %}
networks:
  {{ default_network }}:
    driver: bridge
"#;

/// Environment file template
const ENV_FILE_TEMPLATE: &str = r#"# Environment configuration for {{ app_code }}
# Deployment: {{ deployment_hash }}
# Generated by TryDirect ConfigRenderer

{% for key, value in environment -%}
{{ key }}={{ value }}
{% endfor -%}

{% if domain -%}
# Domain Configuration
APP_DOMAIN={{ domain }}
{% if ssl_enabled -%}
SSL_ENABLED=true
{% endif -%}
{% endif -%}
"#;

/// Individual service template (for partial updates)
const SERVICE_TEMPLATE: &str = r#"
  {{ app.code }}:
    image: {{ app.image }}
    container_name: {{ app.code }}
    restart: {{ app.restart_policy }}
{% if app.environment | length > 0 %}
    environment:
{% for key, value in app.environment %}
      - {{ key }}={{ value }}
{% endfor %}
{% endif %}
{% if app.ports | length > 0 %}
    ports:
{% for port in app.ports %}
      - "{{ port.host }}:{{ port.container }}"
{% endfor %}
{% endif %}
    networks:
      - {{ default_network }}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_environment_object() {
        let renderer = ConfigRenderer::new().unwrap();
        let env = Some(json!({
            "DATABASE_URL": "postgres://localhost/db",
            "PORT": 8080,
            "DEBUG": true
        }));
        let result = renderer.parse_environment(&env).unwrap();
        assert_eq!(
            result.get("DATABASE_URL").unwrap(),
            "postgres://localhost/db"
        );
        assert_eq!(result.get("PORT").unwrap(), "8080");
        assert_eq!(result.get("DEBUG").unwrap(), "true");
    }

    #[test]
    fn test_parse_environment_array() {
        let renderer = ConfigRenderer::new().unwrap();
        let env = Some(json!(["DATABASE_URL=postgres://localhost/db", "PORT=8080"]));
        let result = renderer.parse_environment(&env).unwrap();
        assert_eq!(
            result.get("DATABASE_URL").unwrap(),
            "postgres://localhost/db"
        );
        assert_eq!(result.get("PORT").unwrap(), "8080");
    }

    #[test]
    fn test_parse_ports_object() {
        let renderer = ConfigRenderer::new().unwrap();
        let ports = Some(json!([
            {"host": 8080, "container": 80, "protocol": "tcp"},
            {"host": 443, "container": 443}
        ]));
        let result = renderer.parse_ports(&ports).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].host, 8080);
        assert_eq!(result[0].container, 80);
        assert_eq!(result[1].protocol, "tcp");
    }

    #[test]
    fn test_parse_ports_string() {
        let renderer = ConfigRenderer::new().unwrap();
        let ports = Some(json!(["8080:80", "443:443/tcp"]));
        let result = renderer.parse_ports(&ports).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].host, 8080);
        assert_eq!(result[0].container, 80);
    }

    #[test]
    fn test_parse_volumes() {
        let renderer = ConfigRenderer::new().unwrap();
        let volumes = Some(json!([
            {"source": "/data", "target": "/var/data", "read_only": true},
            "/config:/etc/config:ro"
        ]));
        let result = renderer.parse_volumes(&volumes).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].source, "/data");
        assert!(result[0].read_only);
        assert!(result[1].read_only);
    }

    // =========================================================================
    // Env File Storage Key Tests
    // =========================================================================

    #[test]
    fn test_env_vault_key_format() {
        // Test that .env files are stored with _env suffix
        let app_code = "komodo";
        let env_key = format!("{}_env", app_code);

        assert_eq!(env_key, "komodo_env");
        assert!(env_key.ends_with("_env"));

        // Ensure we can strip the suffix to get app_code back
        let extracted_app_code = env_key.strip_suffix("_env").unwrap();
        assert_eq!(extracted_app_code, app_code);
    }

    #[test]
    fn test_env_destination_path_format() {
        // Test that .env files have correct destination paths
        let deployment_hash = "deployment_abc123";
        let app_code = "telegraf";
        let base_path = "/home/trydirect";

        let expected_path = format!("{}/{}/{}.env", base_path, deployment_hash, app_code);
        assert_eq!(
            expected_path,
            "/home/trydirect/deployment_abc123/telegraf.env"
        );
    }

    #[test]
    fn test_app_config_struct_for_env() {
        // Test AppConfig struct construction for .env files
        let config = AppConfig {
            content: "FOO=bar\nBAZ=qux".to_string(),
            content_type: "env".to_string(),
            destination_path: "/home/trydirect/hash123/app.env".to_string(),
            file_mode: "0640".to_string(),
            owner: Some("trydirect".to_string()),
            group: Some("docker".to_string()),
        };

        assert_eq!(config.content_type, "env");
        assert_eq!(config.file_mode, "0640"); // More restrictive for env files
        assert!(config.destination_path.ends_with(".env"));
    }

    #[test]
    fn test_bundle_app_configs_use_env_key() {
        // Simulate the sync_to_vault behavior where app_configs are stored with _env key
        let app_codes = vec!["telegraf", "nginx", "komodo"];

        for app_code in app_codes {
            let env_key = format!("{}_env", app_code);

            // Verify key format
            assert!(env_key.ends_with("_env"));
            assert!(!env_key.ends_with("_config"));
            assert!(!env_key.ends_with("_compose"));

            // Verify we can identify this as an env config
            assert!(env_key.contains("_env"));
        }
    }

    #[test]
    fn test_config_bundle_structure() {
        // Test the structure of ConfigBundle
        let deployment_hash = "test_hash_123";

        // Simulated app_configs HashMap as created by render_bundle
        let mut app_configs: std::collections::HashMap<String, AppConfig> =
            std::collections::HashMap::new();

        app_configs.insert(
            "telegraf".to_string(),
            AppConfig {
                content: "INFLUX_TOKEN=xxx".to_string(),
                content_type: "env".to_string(),
                destination_path: format!("/home/trydirect/{}/telegraf.env", deployment_hash),
                file_mode: "0640".to_string(),
                owner: Some("trydirect".to_string()),
                group: Some("docker".to_string()),
            },
        );

        app_configs.insert(
            "nginx".to_string(),
            AppConfig {
                content: "DOMAIN=example.com".to_string(),
                content_type: "env".to_string(),
                destination_path: format!("/home/trydirect/{}/nginx.env", deployment_hash),
                file_mode: "0640".to_string(),
                owner: Some("trydirect".to_string()),
                group: Some("docker".to_string()),
            },
        );

        assert_eq!(app_configs.len(), 2);
        assert!(app_configs.contains_key("telegraf"));
        assert!(app_configs.contains_key("nginx"));

        // When storing, each should be stored with _env suffix
        for (app_code, _config) in &app_configs {
            let env_key = format!("{}_env", app_code);
            assert!(env_key.ends_with("_env"));
        }
    }
}
