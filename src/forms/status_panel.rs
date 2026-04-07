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

fn default_list_include_health() -> bool {
    true
}

fn default_list_log_lines() -> usize {
    10
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
    /// App code to check health for. Use "all" or omit to get all containers.
    #[serde(default = "default_health_app_code")]
    pub app_code: String,
    /// Optional container/service name override
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default = "default_include_metrics")]
    pub include_metrics: bool,
    /// When true and app_code is "system" or empty, return system containers (status_panel, compose-agent)
    #[serde(default)]
    pub include_system: bool,
}

fn default_health_app_code() -> String {
    "all".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LogsCommandRequest {
    pub app_code: String,
    /// Optional container/service name override
    #[serde(default)]
    pub container: Option<String>,
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
pub struct ListContainersCommandRequest {
    #[serde(default = "default_list_include_health")]
    pub include_health: bool,
    #[serde(default)]
    pub include_logs: bool,
    #[serde(default = "default_list_log_lines")]
    pub log_lines: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RestartCommandRequest {
    pub app_code: String,
    /// Optional container/service name override
    #[serde(default)]
    pub container: Option<String>,
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
    /// Container runtime to use: "runc" (default) or "kata"
    #[serde(default = "default_runtime")]
    pub runtime: String,
}

fn default_deploy_pull() -> bool {
    true
}

fn default_runtime() -> String {
    "runc".to_string()
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

fn default_firewall_action() -> String {
    "add".to_string()
}

fn default_firewall_protocol() -> String {
    "tcp".to_string()
}

fn default_firewall_source() -> String {
    "0.0.0.0/0".to_string()
}

/// Request to configure iptables firewall rules on the target server
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConfigureFirewallCommandRequest {
    /// App code for context (optional, used for logging/tracking)
    #[serde(default)]
    pub app_code: Option<String>,
    /// Public ports to open (accessible from any IP)
    #[serde(default)]
    pub public_ports: Vec<FirewallPortRule>,
    /// Private ports to open (restricted to specific IPs/networks)
    #[serde(default)]
    pub private_ports: Vec<FirewallPortRule>,
    /// Action: "add", "remove", "list", "flush"
    #[serde(default = "default_firewall_action")]
    pub action: String,
    /// Whether to persist rules across reboots (default: true)
    #[serde(default = "default_persist_rules")]
    pub persist: bool,
}

fn default_persist_rules() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FirewallPortRule {
    /// Port number (e.g., 80, 443, 5432)
    pub port: u16,
    /// Protocol: "tcp" or "udp"
    #[serde(default = "default_firewall_protocol")]
    pub protocol: String,
    /// Source IP/CIDR (e.g., "0.0.0.0/0" for any, "10.0.0.0/8" for internal)
    #[serde(default = "default_firewall_source")]
    pub source: String,
    /// Optional description/comment for the rule
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConfigureFirewallCommandReport {
    #[serde(rename = "type")]
    pub command_type: String,
    pub deployment_hash: String,
    #[serde(default)]
    pub app_code: Option<String>,
    pub status: FirewallStatus,
    /// Rules that were applied/removed/listed
    #[serde(default)]
    pub rules: Vec<FirewallRuleResult>,
    #[serde(default)]
    pub errors: Vec<StatusPanelCommandError>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum FirewallStatus {
    Ok,
    PartialSuccess,
    Failed,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FirewallRuleResult {
    pub port: u16,
    pub protocol: String,
    pub source: String,
    pub applied: bool,
    #[serde(default)]
    pub message: Option<String>,
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
    // Allow "all" as a special value for health checks
    if app_code != "all" {
        ensure_app_code(expected_type, app_code)?;
    }
    Ok(())
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
            // Allow "all" as a special value to get all containers' health
            if params.app_code != "all" {
                ensure_app_code("health", &params.app_code)?;
            }

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
        "list_containers" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let params: ListContainersCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid list_containers parameters: {}", err))?;

            if params.include_logs && (params.log_lines == 0 || params.log_lines > 100) {
                return Err("list_containers.log_lines must be between 1 and 100".to_string());
            }

            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode list_containers parameters: {}", err))
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

            // Validate runtime
            let allowed_runtimes = ["runc", "kata"];
            if !allowed_runtimes.contains(&params.runtime.as_str()) {
                return Err(format!(
                    "deploy_app: runtime must be one of: {}; got '{}'",
                    allowed_runtimes.join(", "),
                    params.runtime
                ));
            }

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
        "configure_firewall" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let params: ConfigureFirewallCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid configure_firewall parameters: {}", err))?;

            // Validate action
            if !["add", "remove", "list", "flush"].contains(&params.action.as_str()) {
                return Err(
                    "configure_firewall: action must be one of: add, remove, list, flush"
                        .to_string(),
                );
            }

            // Validate port rules
            for rule in params.public_ports.iter().chain(params.private_ports.iter()) {
                if rule.port == 0 {
                    return Err("configure_firewall: port must be > 0".to_string());
                }
                if !["tcp", "udp"].contains(&rule.protocol.as_str()) {
                    return Err(
                        "configure_firewall: protocol must be one of: tcp, udp".to_string(),
                    );
                }
            }

            // For add/remove, require at least one port rule (unless flush/list)
            if ["add", "remove"].contains(&params.action.as_str())
                && params.public_ports.is_empty()
                && params.private_ports.is_empty()
            {
                return Err(
                    "configure_firewall: at least one public_port or private_port is required for add/remove actions"
                        .to_string(),
                );
            }

            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode configure_firewall parameters: {}", err))
        }
        "probe_endpoints" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let params: ProbeEndpointsCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid probe_endpoints parameters: {}", err))?;
            ensure_app_code("probe_endpoints", &params.app_code)?;

            let valid_protocols = ["openapi", "html_forms", "graphql", "mcp", "rest"];
            for p in &params.protocols {
                if !valid_protocols.contains(&p.as_str()) {
                    return Err(format!(
                        "probe_endpoints: unsupported protocol '{}'. Valid: {:?}",
                        p, valid_protocols
                    ));
                }
            }

            if params.probe_timeout == 0 || params.probe_timeout > 30 {
                return Err("probe_endpoints: probe_timeout must be between 1 and 30".to_string());
            }

            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode probe_endpoints parameters: {}", err))
        }
        "check_connections" => {
            let value = parameters.clone().unwrap_or_else(|| json!({}));
            let params: CheckConnectionsCommandRequest = serde_json::from_value(value)
                .map_err(|err| format!("Invalid check_connections parameters: {}", err))?;
            serde_json::to_value(params)
                .map(Some)
                .map_err(|err| format!("Failed to encode check_connections parameters: {}", err))
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
        "configure_firewall" => {
            let value = result
                .clone()
                .ok_or_else(|| "configure_firewall result payload is required".to_string())?;
            let report: ConfigureFirewallCommandReport = serde_json::from_value(value)
                .map_err(|err| format!("Invalid configure_firewall result: {}", err))?;

            if report.command_type != "configure_firewall" {
                return Err("configure_firewall result must include type='configure_firewall'".to_string());
            }
            if report.deployment_hash != deployment_hash {
                return Err("configure_firewall result deployment_hash mismatch".to_string());
            }

            serde_json::to_value(report)
                .map(Some)
                .map_err(|err| format!("Failed to encode configure_firewall result: {}", err))
        }
        "probe_endpoints" => {
            let value = result
                .clone()
                .ok_or_else(|| "probe_endpoints result payload is required".to_string())?;
            let report: ProbeEndpointsCommandReport = serde_json::from_value(value)
                .map_err(|err| format!("Invalid probe_endpoints result: {}", err))?;

            if report.command_type != "probe_endpoints" {
                return Err("probe_endpoints result must include type='probe_endpoints'".to_string());
            }
            if report.deployment_hash != deployment_hash {
                return Err("probe_endpoints result deployment_hash mismatch".to_string());
            }

            serde_json::to_value(report)
                .map(Some)
                .map_err(|err| format!("Failed to encode probe_endpoints result: {}", err))
        }
        _ => Ok(result.clone()),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Pipe: probe_endpoints
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn default_probe_protocols() -> Vec<String> {
    vec!["openapi".to_string(), "rest".to_string()]
}

fn default_probe_timeout() -> u32 {
    5
}

/// Request to probe a container for connectable API endpoints
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProbeEndpointsCommandRequest {
    /// App code to probe
    pub app_code: String,
    /// Optional container/service name override
    #[serde(default)]
    pub container: Option<String>,
    /// Protocols to probe: "openapi", "html_forms", "graphql", "mcp", "rest"
    #[serde(default = "default_probe_protocols")]
    pub protocols: Vec<String>,
    /// Timeout per probe request in seconds
    #[serde(default = "default_probe_timeout")]
    pub probe_timeout: u32,
}

/// A discovered API endpoint
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProbeEndpoint {
    pub protocol: String,
    pub base_url: String,
    pub spec_url: String,
    pub operations: Vec<ProbeOperation>,
}

/// A single API operation (path + method + fields)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProbeOperation {
    pub path: String,
    pub method: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub fields: Vec<String>,
}

/// A discovered HTML form
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProbeForm {
    pub id: String,
    pub action: String,
    pub method: String,
    pub fields: Vec<String>,
}

/// Request parameters for the `check_connections` command.
///
/// All fields are optional — when `ports` is omitted the agent checks the
/// common HTTP/HTTPS ports (80, 443, 8080, 3000, 8443).
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CheckConnectionsCommandRequest {
    /// Specific TCP ports to check for active connections.
    /// Defaults to the common HTTP/HTTPS set when not provided.
    #[serde(default)]
    pub ports: Option<Vec<u16>>,
}

/// Result of probing a container for endpoints
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProbeEndpointsCommandReport {
    #[serde(rename = "type")]
    pub command_type: String,
    pub deployment_hash: String,
    pub app_code: String,
    pub protocols_detected: Vec<String>,
    pub endpoints: Vec<ProbeEndpoint>,
    pub forms: Vec<ProbeForm>,
    pub probed_at: String,
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
    fn list_containers_defaults_apply() {
        let params = validate_command_parameters("list_containers", &Some(json!({})))
            .expect("list_containers params should validate")
            .expect("list_containers params must be present");

        assert_eq!(params["include_health"], true);
        assert_eq!(params["include_logs"], false);
        assert_eq!(params["log_lines"], 10);
    }

    #[test]
    fn list_containers_log_lines_validate() {
        let err = validate_command_parameters(
            "list_containers",
            &Some(json!({
                "include_logs": true,
                "log_lines": 0
            })),
        )
        .expect_err("invalid log_lines should fail");

        assert!(err.contains("log_lines"));
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

    #[test]
    fn firewall_parameters_validate_action() {
        let err = validate_command_parameters(
            "configure_firewall",
            &Some(json!({
                "action": "invalid_action",
                "public_ports": [{"port": 80}]
            })),
        )
        .expect_err("invalid action should fail");

        assert!(err.contains("action must be one of"));
    }

    #[test]
    fn firewall_parameters_validate_port() {
        let err = validate_command_parameters(
            "configure_firewall",
            &Some(json!({
                "action": "add",
                "public_ports": [{"port": 0, "protocol": "tcp"}]
            })),
        )
        .expect_err("port 0 should fail");

        assert!(err.contains("port must be > 0"));
    }

    #[test]
    fn firewall_parameters_validate_protocol() {
        let err = validate_command_parameters(
            "configure_firewall",
            &Some(json!({
                "action": "add",
                "public_ports": [{"port": 80, "protocol": "invalid"}]
            })),
        )
        .expect_err("invalid protocol should fail");

        assert!(err.contains("protocol must be one of"));
    }

    #[test]
    fn firewall_parameters_require_ports_for_add() {
        let err = validate_command_parameters(
            "configure_firewall",
            &Some(json!({
                "action": "add"
            })),
        )
        .expect_err("add without ports should fail");

        assert!(err.contains("at least one public_port or private_port"));
    }

    #[test]
    fn firewall_parameters_list_does_not_require_ports() {
        let result = validate_command_parameters(
            "configure_firewall",
            &Some(json!({
                "action": "list"
            })),
        )
        .expect("list without ports should succeed");

        assert!(result.is_some());
    }

    #[test]
    fn firewall_parameters_valid_public_port() {
        let result = validate_command_parameters(
            "configure_firewall",
            &Some(json!({
                "action": "add",
                "public_ports": [
                    {"port": 80, "protocol": "tcp", "source": "0.0.0.0/0"},
                    {"port": 443, "protocol": "tcp"}
                ]
            })),
        )
        .expect("valid public ports should succeed")
        .expect("params should be present");

        assert_eq!(result["action"], "add");
        assert_eq!(result["public_ports"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn firewall_parameters_valid_private_port() {
        let result = validate_command_parameters(
            "configure_firewall",
            &Some(json!({
                "action": "add",
                "private_ports": [
                    {"port": 5432, "protocol": "tcp", "source": "10.0.0.0/8"}
                ]
            })),
        )
        .expect("valid private ports should succeed")
        .expect("params should be present");

        assert_eq!(result["action"], "add");
        assert_eq!(result["private_ports"].as_array().unwrap().len(), 1);
    }

    // ── probe_endpoints tests ────────────────────────────

    #[test]
    fn probe_endpoints_parameters_defaults() {
        let params = validate_command_parameters(
            "probe_endpoints",
            &Some(json!({
                "app_code": "crm"
            })),
        )
        .expect("probe_endpoints params should validate")
        .expect("probe_endpoints params must be present");

        assert_eq!(params["app_code"], "crm");
        assert_eq!(params["protocols"], json!(["openapi", "rest"]));
        assert_eq!(params["probe_timeout"], 5);
    }

    #[test]
    fn probe_endpoints_parameters_require_app_code() {
        let err = validate_command_parameters(
            "probe_endpoints",
            &Some(json!({
                "protocols": ["openapi"]
            })),
        )
        .expect_err("missing app_code should fail");

        assert!(err.contains("app_code"));
    }

    #[test]
    fn probe_endpoints_parameters_reject_invalid_protocol() {
        let err = validate_command_parameters(
            "probe_endpoints",
            &Some(json!({
                "app_code": "crm",
                "protocols": ["openapi", "invalid_proto"]
            })),
        )
        .expect_err("invalid protocol should fail");

        assert!(err.contains("unsupported protocol"));
    }

    #[test]
    fn probe_endpoints_parameters_reject_zero_timeout() {
        let err = validate_command_parameters(
            "probe_endpoints",
            &Some(json!({
                "app_code": "crm",
                "probe_timeout": 0
            })),
        )
        .expect_err("zero timeout should fail");

        assert!(err.contains("probe_timeout"));
    }

    #[test]
    fn probe_endpoints_parameters_reject_excessive_timeout() {
        let err = validate_command_parameters(
            "probe_endpoints",
            &Some(json!({
                "app_code": "crm",
                "probe_timeout": 31
            })),
        )
        .expect_err("excessive timeout should fail");

        assert!(err.contains("probe_timeout"));
    }

    #[test]
    fn probe_endpoints_result_validates_type() {
        let err = validate_command_result(
            "probe_endpoints",
            "hash_a",
            &Some(json!({
                "type": "wrong_type",
                "deployment_hash": "hash_a",
                "app_code": "crm",
                "protocols_detected": [],
                "endpoints": [],
                "forms": [],
                "probed_at": "2026-03-20T12:00:00Z"
            })),
        )
        .expect_err("wrong type should fail");

        assert!(err.contains("type='probe_endpoints'"));
    }

    #[test]
    fn probe_endpoints_result_validates_hash() {
        let err = validate_command_result(
            "probe_endpoints",
            "hash_a",
            &Some(json!({
                "type": "probe_endpoints",
                "deployment_hash": "hash_b",
                "app_code": "crm",
                "protocols_detected": [],
                "endpoints": [],
                "forms": [],
                "probed_at": "2026-03-20T12:00:00Z"
            })),
        )
        .expect_err("mismatched hash should fail");

        assert!(err.contains("deployment_hash mismatch"));
    }

    #[test]
    fn probe_endpoints_result_valid() {
        let result = validate_command_result(
            "probe_endpoints",
            "hash_a",
            &Some(json!({
                "type": "probe_endpoints",
                "deployment_hash": "hash_a",
                "app_code": "crm",
                "protocols_detected": ["openapi"],
                "endpoints": [{
                    "protocol": "openapi",
                    "base_url": "http://crm:80",
                    "spec_url": "/swagger.json",
                    "operations": [{
                        "path": "/api/v1/contacts",
                        "method": "POST",
                        "summary": "Create contact",
                        "fields": ["last_name", "email1"]
                    }]
                }],
                "forms": [],
                "probed_at": "2026-03-20T12:00:00Z"
            })),
        )
        .expect("valid result should pass");

        assert!(result.is_some());
    }

    // ── check_connections ────────────────────────────────────────────

    #[test]
    fn check_connections_accepts_no_parameters() {
        let result = validate_command_parameters("check_connections", &None)
            .expect("check_connections with no params should validate");
        // Result may be Some({}) or None — both are acceptable
        if let Some(v) = result {
            assert!(v.is_object(), "result must be an object when present");
        }
    }

    #[test]
    fn check_connections_accepts_empty_object() {
        let result = validate_command_parameters("check_connections", &Some(json!({})))
            .expect("check_connections with empty object should validate");
        if let Some(v) = result {
            assert!(v.is_object());
        }
    }

    #[test]
    fn check_connections_accepts_port_list() {
        let result = validate_command_parameters(
            "check_connections",
            &Some(json!({ "ports": [80, 443, 8080] })),
        )
        .expect("check_connections with port list should validate");
        let v = result.expect("result must be present");
        let ports = v["ports"].as_array().expect("ports must be an array");
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0], 80);
    }

    #[test]
    fn check_connections_accepts_null_ports() {
        let result = validate_command_parameters(
            "check_connections",
            &Some(json!({ "ports": null })),
        )
        .expect("check_connections with null ports should validate");
        assert!(result.is_some());
    }

    #[test]
    fn deploy_app_defaults_runtime_to_runc() {
        let params = json!({"app_code": "web"});
        let result = validate_command_parameters("deploy_app", &Some(params)).unwrap();
        let val = result.unwrap();
        assert_eq!(val["runtime"], "runc");
    }

    #[test]
    fn deploy_app_accepts_kata_runtime() {
        let params = json!({"app_code": "web", "runtime": "kata"});
        let result = validate_command_parameters("deploy_app", &Some(params)).unwrap();
        let val = result.unwrap();
        assert_eq!(val["runtime"], "kata");
    }

    #[test]
    fn deploy_app_accepts_runc_runtime() {
        let params = json!({"app_code": "web", "runtime": "runc"});
        let result = validate_command_parameters("deploy_app", &Some(params)).unwrap();
        let val = result.unwrap();
        assert_eq!(val["runtime"], "runc");
    }

    #[test]
    fn deploy_app_rejects_unknown_runtime() {
        let params = json!({"app_code": "web", "runtime": "containerd"});
        let result = validate_command_parameters("deploy_app", &Some(params));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("runtime must be one of"));
    }
}
