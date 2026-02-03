use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

fn default_include_metrics() -> bool {
    true
}

fn default_log_limit() -> i32 {
    400
}

fn default_log_streams() -> Vec<String> {
    vec!["stdout".to_string(), "stderr".to_string()]
}

fn default_log_redact() -> bool {
    true
}

fn default_delete_config() -> bool {
    true
}

fn default_restart_force() -> bool {
    false
}

fn default_ssl_enabled() -> bool {
    true
}

fn default_create_action() -> String {
    "create".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HealthCommandRequest {
    pub app_code: String,
    #[serde(default = "default_include_metrics")]
    pub include_metrics: bool,
    /// When true and app_code is "system" or empty, return system containers (status_panel, compose-agent)
    #[serde(default)]
    pub include_system: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LogsCommandRequest {
    pub app_code: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_log_limit")]
    pub limit: i32,
    #[serde(default = "default_log_streams")]
    pub streams: Vec<String>,
    #[serde(default = "default_log_redact")]
    pub redact: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RestartCommandRequest {
    pub app_code: String,
    #[serde(default = "default_restart_force")]
    pub force: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployAppCommandRequest {
    pub app_code: String,
    /// Optional: docker-compose.yml content (generated from J2 template)
    /// If provided, will be written to disk before deploying
    #[serde(default)]
    pub compose_content: Option<String>,
    /// Optional: specific image to use (overrides compose file)
    #[serde(default)]
    pub image: Option<String>,
    /// Optional: environment variables to set
    #[serde(default)]
    pub env_vars: Option<std::collections::HashMap<String, String>>,
    /// Whether to pull the image before starting (default: true)
    #[serde(default = "default_deploy_pull")]
    pub pull: bool,
    /// Whether to remove existing container before deploying
    #[serde(default)]
    pub force_recreate: bool,
}

fn default_deploy_pull() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RemoveAppCommandRequest {
    pub app_code: String,
    #[serde(default = "default_delete_config")]
    pub delete_config: bool,
    #[serde(default)]
    pub remove_volumes: bool,
    #[serde(default)]
    pub remove_image: bool,
}

/// Request to configure nginx proxy manager for an app
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConfigureProxyCommandRequest {
    pub app_code: String,
    /// Domain name(s) to proxy (e.g., ["komodo.example.com"])
    pub domain_names: Vec<String>,
    /// Container/service name to forward to (defaults to app_code)
    #[serde(default)]
    pub forward_host: Option<String>,
    /// Port on the container to forward to
    pub forward_port: u16,
    /// Enable SSL with Let's Encrypt
    #[serde(default = "default_ssl_enabled")]
    pub ssl_enabled: bool,
    /// Force HTTPS redirect
    #[serde(default = "default_ssl_enabled")]
    pub ssl_forced: bool,
    /// HTTP/2 support
    #[serde(default = "default_ssl_enabled")]
    pub http2_support: bool,
    /// Action: "create", "update", "delete"
    #[serde(default = "default_create_action")]
    pub action: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Ok,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ContainerState {
    Running,
    Exited,
    Starting,
    Failed,
    Unknown,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HealthCommandReport {
    #[serde(rename = "type")]
    pub command_type: String,
    pub deployment_hash: String,
    pub app_code: String,
    pub status: HealthStatus,
    pub container_state: ContainerState,
    #[serde(default)]
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub metrics: Option<Value>,
    #[serde(default)]
    pub errors: Vec<StatusPanelCommandError>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LogLine {
    pub ts: DateTime<Utc>,
    pub stream: LogStream,
    pub message: String,
    #[serde(default)]
    pub redacted: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LogsCommandReport {
    #[serde(rename = "type")]
    pub command_type: String,
    pub deployment_hash: String,
    pub app_code: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub lines: Vec<LogLine>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum RestartStatus {
    Ok,
    Failed,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RestartCommandReport {
    #[serde(rename = "type")]
    pub command_type: String,
    pub deployment_hash: String,
    pub app_code: String,
    pub status: RestartStatus,
    pub container_state: ContainerState,
    #[serde(default)]
    pub errors: Vec<StatusPanelCommandError>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StatusPanelCommandError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: Option<Value>,
}

fn ensure_app_code(kind: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{}.app_code is required", kind));
    }
    Ok(())
}

fn ensure_result_envelope(
    expected_type: &str,
    expected_hash: &str,
    actual_type: &str,
    actual_hash: &str,
    app_code: &str,
) -> Result<(), String> {
    if actual_type != expected_type {
        return Err(format!(
            "{} result must include type='{}'",
            expected_type, expected_type
        ));
    }
    if actual_hash != expected_hash {
        return Err(format!("{} result deployment_hash mismatch", expected_type));
    }
    ensure_app_code(expected_type, app_code)
}

pub fn validate_command_parameters(
    command_type: &str,
    parameters: &Option<Value>,
) -> Result<Option<Value>, String> {
    match command_type {
        "health" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let params: HealthCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid health parameters: {}", err))?;
            ensure_app_code("health", &params.app_code)?;

            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode health parameters: {}", err))
        }
        "logs" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let mut params: LogsCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid logs parameters: {}", err))?;
            ensure_app_code("logs", &params.app_code)?;

            if params.limit <= 0 || params.limit > 1000 {
                return Err("logs.limit must be between 1 and 1000".to_string());
            }

            if params.streams.is_empty() {
                params.streams = default_log_streams();
            }

            let allowed_streams = ["stdout", "stderr"];
            if !params
                .streams
                .iter()
                .all(|s| allowed_streams.contains(&s.as_str()))
            {
                return Err("logs.streams must be one of: stdout, stderr".to_string());
            }

            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode logs parameters: {}", err))
        }
        "restart" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let params: RestartCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid restart parameters: {}", err))?;
            ensure_app_code("restart", &params.app_code)?;

            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode restart parameters: {}", err))
        }
        "deploy_app" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let params: DeployAppCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid deploy_app parameters: {}", err))?;
            ensure_app_code("deploy_app", &params.app_code)?;

            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode deploy_app parameters: {}", err))
        }
        "remove_app" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let params: RemoveAppCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid remove_app parameters: {}", err))?;
            ensure_app_code("remove_app", &params.app_code)?;

            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode remove_app parameters: {}", err))
        }
        "configure_proxy" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let params: ConfigureProxyCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid configure_proxy parameters: {}", err))?;
            ensure_app_code("configure_proxy", &params.app_code)?;

            // Validate required fields
            if params.domain_names.is_empty() {
                return Err("configure_proxy: at least one domain_name is required".to_string());
            }
            if params.forward_port == 0 {
                return Err("configure_proxy: forward_port is required and must be > 0".to_string());
            }
            if !["create", "update", "delete"].contains(&params.action.as_str()) {
                return Err(
                    "configure_proxy: action must be one of: create, update, delete".to_string(),
                );
            }

            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode configure_proxy parameters: {}", err))
        }
        _ => Ok(parameters.clone()),
    }
}

pub fn validate_command_result(
    command_type: &str,
    deployment_hash: &str,
    result: &Option<Value>,
) -> Result<Option<Value>, String> {
    match command_type {
        "health" => {
            let value = result
                .clone()
                .ok_or_else(|| "health result payload is required".to_string())?;
            let report: HealthCommandReport = serde_json::from_value(value)
                .map_err(|err| format!("Invalid health result: {}", err))?;

            ensure_result_envelope(
                "health",
                deployment_hash,
                &report.command_type,
                &report.deployment_hash,
                &report.app_code,
            )?;

            if let Some(metrics) = report.metrics.as_ref() {
                if !metrics.is_object() {
                    return Err("health.metrics must be an object".to_string());
                }
            }

            serde_json::to_value(report)
                .map(Some)
                .map_err(|err| format!("Failed to encode health result: {}", err))
        }
        "logs" => {
            let value = result
                .clone()
                .ok_or_else(|| "logs result payload is required".to_string())?;
            let report: LogsCommandReport = serde_json::from_value(value)
                .map_err(|err| format!("Invalid logs result: {}", err))?;

            ensure_result_envelope(
                "logs",
                deployment_hash,
                &report.command_type,
                &report.deployment_hash,
                &report.app_code,
            )?;

            serde_json::to_value(report)
                .map(Some)
                .map_err(|err| format!("Failed to encode logs result: {}", err))
        }
        "restart" => {
            let value = result
                .clone()
                .ok_or_else(|| "restart result payload is required".to_string())?;
            let report: RestartCommandReport = serde_json::from_value(value)
                .map_err(|err| format!("Invalid restart result: {}", err))?;

            ensure_result_envelope(
                "restart",
                deployment_hash,
                &report.command_type,
                &report.deployment_hash,
                &report.app_code,
            )?;

            serde_json::to_value(report)
                .map(Some)
                .map_err(|err| format!("Failed to encode restart result: {}", err))
        }
        _ => Ok(result.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_parameters_apply_defaults() {
        let params = validate_command_parameters(
            "health",
            &Some(json!({
                "app_code": "web"
            })),
        )
        .expect("health params should validate")
        .expect("health params must be present");

        assert_eq!(params["app_code"], "web");
        assert_eq!(params["include_metrics"], true);
    }

    #[test]
    fn logs_parameters_validate_streams() {
        let err = validate_command_parameters(
            "logs",
            &Some(json!({
                "app_code": "api",
                "streams": ["stdout", "weird"]
            })),
        )
        .expect_err("invalid stream should fail");

        assert!(err.contains("logs.streams"));
    }

    #[test]
    fn health_result_requires_matching_hash() {
        let err = validate_command_result(
            "health",
            "hash_a",
            &Some(json!({
                "type": "health",
                "deployment_hash": "hash_b",
                "app_code": "web",
                "status": "ok",
                "container_state": "running",
                "errors": []
            })),
        )
        .expect_err("mismatched hash should fail");

        assert!(err.contains("deployment_hash"));
    }
}
