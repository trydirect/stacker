use crate::cli::config_parser::{DomainConfig, SslMode};
use crate::cli::error::CliError;
use crate::cli::proxy_manager::{
    ContainerRuntime, DockerCliRuntime, ProxyDetection, detect_proxy, generate_nginx_server_block,
};
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

/// `stacker proxy detect`
///
/// Scans running containers for an existing reverse-proxy (nginx, traefik, etc.)
/// and reports what was found.
pub struct ProxyDetectCommand;

impl ProxyDetectCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CallableTrait for ProxyDetectCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let runtime = DockerCliRuntime;
        let detection = run_detect(&runtime)?;

        eprintln!("Detected proxy: {:?}", detection.proxy_type);
        if let Some(name) = &detection.container_name {
            eprintln!("  Container: {}", name);
        }
        if !detection.ports.is_empty() {
            eprintln!("  Ports: {:?}", detection.ports);
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
