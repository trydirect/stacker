use crate::cli::config_parser::{
    CloudOrchestrator, DeployTarget, DomainConfig, SslMode, StackerConfig,
};
use crate::cli::deployment_lock::DeploymentLock;
use crate::cli::error::CliError;
use crate::cli::proxy_manager::{
    ContainerRuntime, DockerCliRuntime, ProxyDetection, detect_proxy,
    detect_proxy_from_snapshot, generate_nginx_server_block,
};
use crate::cli::runtime::CliRuntime;
use crate::console::commands::CallableTrait;

/// Parse SSL mode string to `SslMode` enum.
pub fn parse_ssl_mode(s: Option<&str>) -> SslMode {
    match s.map(|v| v.to_lowercase()).as_deref() {
        Some("auto") => SslMode::Auto,
        Some("manual") => SslMode::Manual,
        _ => SslMode::Off,
    }
}

/// Build a `DomainConfig` from CLI arguments.
pub fn build_domain_config(domain: &str, upstream: Option<&str>, ssl: Option<&str>) -> DomainConfig {
    DomainConfig {
        domain: domain.to_string(),
        ssl: parse_ssl_mode(ssl),
        upstream: upstream.unwrap_or("http://app:8080").to_string(),
    }
}

/// Run proxy detection using a `ContainerRuntime` (DIP).
pub fn run_detect(runtime: &dyn ContainerRuntime) -> Result<ProxyDetection, CliError> {
    detect_proxy(runtime)
}

/// `stacker proxy add <domain> [--upstream <host:port>] [--ssl auto|manual|off]`
///
/// Adds a reverse-proxy entry for the given domain.
pub struct ProxyAddCommand {
    pub domain: String,
    pub upstream: Option<String>,
    pub ssl: Option<String>,
}

impl ProxyAddCommand {
    pub fn new(domain: String, upstream: Option<String>, ssl: Option<String>) -> Self {
        Self {
            domain,
            upstream,
            ssl,
        }
    }
}

impl CallableTrait for ProxyAddCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config = build_domain_config(
            &self.domain,
            self.upstream.as_deref(),
            self.ssl.as_deref(),
        );
        let block = generate_nginx_server_block(&config);
        println!("{}", block);
        eprintln!("✓ Proxy entry generated for {}", self.domain);
        Ok(())
    }
}

/// `stacker proxy detect [--json] [--deployment <hash>]`
///
/// Scans running containers for an existing reverse-proxy (nginx, traefik, etc.)
/// and reports what was found.
///
/// - **Local deployments**: runs `docker ps` locally.
/// - **Cloud/remote deployments**: queries the Status Panel agent snapshot.
pub struct ProxyDetectCommand {
    pub json: bool,
    pub deployment: Option<String>,
}

impl ProxyDetectCommand {
    pub fn new(json: bool, deployment: Option<String>) -> Self {
        Self { json, deployment }
    }
}

/// Check whether the current project is configured for cloud/remote deployment.
fn is_cloud_or_remote(project_dir: &std::path::Path) -> bool {
    // 1. Check deployment lock
    if let Ok(Some(lock)) = DeploymentLock::load(project_dir) {
        if lock.target == "cloud" || lock.target == "server" {
            return true;
        }
    }

    // 2. Check stacker.yml
    let config_path = project_dir.join("stacker.yml");
    if let Ok(config_str) = std::fs::read_to_string(&config_path) {
        if let Ok(config) = serde_yaml::from_str::<StackerConfig>(&config_str) {
            if config.deploy.target == DeployTarget::Cloud {
                return true;
            }
            if config.deploy.target == DeployTarget::Server {
                return true;
            }
            if let Some(cloud_cfg) = &config.deploy.cloud {
                if cloud_cfg.orchestrator == CloudOrchestrator::Remote {
                    return true;
                }
            }
        }
    }

    false
}

/// Resolve deployment hash for proxy detection (minimal version).
fn resolve_deployment_hash_for_proxy(
    explicit: &Option<String>,
    ctx: &CliRuntime,
) -> Result<String, CliError> {
    if let Some(hash) = explicit {
        if !hash.is_empty() {
            return Ok(hash.clone());
        }
    }

    let project_dir = std::env::current_dir().map_err(CliError::Io)?;

    if let Some(lock) = DeploymentLock::load(&project_dir)? {
        if let Some(dep_id) = lock.deployment_id {
            let info = ctx.block_on(ctx.client.get_deployment_status(dep_id as i32))?;
            if let Some(info) = info {
                return Ok(info.deployment_hash);
            }
        }
    }

    let config_path = project_dir.join("stacker.yml");
    if config_path.exists() {
        if let Ok(config) = StackerConfig::from_file(&config_path) {
            if let Some(ref project_name) = config.project.identity {
                let project = ctx.block_on(ctx.client.find_project_by_name(project_name))?;
                if let Some(proj) = project {
                    let dep = ctx.block_on(ctx.client.get_deployment_status_by_project(proj.id))?;
                    if let Some(dep) = dep {
                        return Ok(dep.deployment_hash);
                    }
                }
            }
        }
    }

    Err(CliError::ConfigValidation(
        "Cannot determine deployment hash for remote proxy detection.\n\
         Use --deployment <HASH>, or run from a directory with a deployment lock or stacker.yml."
            .to_string(),
    ))
}

/// Pretty-print a proxy detection result.
fn print_detection(detection: &ProxyDetection, json: bool) {
    if json {
        let val = serde_json::json!({
            "proxy_type": format!("{:?}", detection.proxy_type),
            "container_name": detection.container_name,
            "ports": detection.ports,
        });
        println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
        return;
    }

    eprintln!("Detected proxy: {:?}", detection.proxy_type);
    if let Some(name) = &detection.container_name {
        eprintln!("  Container: {}", name);
    }
    if !detection.ports.is_empty() {
        eprintln!("  Ports: {:?}", detection.ports);
    }
}

impl CallableTrait for ProxyDetectCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = std::env::current_dir()?;

        // If an explicit --deployment flag was given, or the project is
        // deployed to cloud/server, use the agent snapshot for detection.
        let use_remote = self.deployment.is_some() || is_cloud_or_remote(&project_dir);

        if use_remote {
            let ctx = CliRuntime::new("proxy detect")?;
            let hash = resolve_deployment_hash_for_proxy(&self.deployment, &ctx)?;

            let snapshot = ctx.block_on(ctx.client.agent_snapshot(&hash))?;
            let detection = detect_proxy_from_snapshot(&snapshot);
            print_detection(&detection, self.json);
        } else {
            let runtime = DockerCliRuntime;
            let detection = run_detect(&runtime)?;
            print_detection(&detection, self.json);
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::config_parser::ProxyType;
    use crate::cli::proxy_manager::ContainerInfo;

    struct MockRuntime {
        containers: Vec<ContainerInfo>,
    }

    impl ContainerRuntime for MockRuntime {
        fn list_containers(&self) -> Result<Vec<ContainerInfo>, CliError> {
            Ok(self.containers.clone())
        }
        fn is_available(&self) -> bool {
            true
        }
    }

    #[test]
    fn test_parse_ssl_mode_auto() {
        assert_eq!(parse_ssl_mode(Some("auto")), SslMode::Auto);
        assert_eq!(parse_ssl_mode(Some("AUTO")), SslMode::Auto);
    }

    #[test]
    fn test_parse_ssl_mode_defaults_to_off() {
        assert_eq!(parse_ssl_mode(None), SslMode::Off);
        assert_eq!(parse_ssl_mode(Some("unknown")), SslMode::Off);
    }

    #[test]
    fn test_build_domain_config_with_defaults() {
        let cfg = build_domain_config("example.com", None, None);
        assert_eq!(cfg.domain, "example.com");
        assert_eq!(cfg.upstream, "http://app:8080");
        assert_eq!(cfg.ssl, SslMode::Off);
    }

    #[test]
    fn test_build_domain_config_with_overrides() {
        let cfg = build_domain_config("app.io", Some("http://web:3000"), Some("auto"));
        assert_eq!(cfg.upstream, "http://web:3000");
        assert_eq!(cfg.ssl, SslMode::Auto);
    }

    #[test]
    fn test_detect_returns_none_for_empty_containers() {
        let runtime = MockRuntime { containers: vec![] };
        let result = run_detect(&runtime).unwrap();
        assert_eq!(result.proxy_type, ProxyType::None);
    }

    #[test]
    fn test_detect_finds_nginx_proxy() {
        let runtime = MockRuntime {
            containers: vec![ContainerInfo {
                id: "abc123".to_string(),
                name: "nginx-1".to_string(),
                image: "nginx:latest".to_string(),
                ports: vec![80, 443],
                status: "running".to_string(),
            }],
        };
        let result = run_detect(&runtime).unwrap();
        assert_eq!(result.proxy_type, ProxyType::Nginx);
    }
}
