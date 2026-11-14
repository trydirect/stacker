use crate::cli::config_parser::{DomainConfig, ProxyType, SslMode, StackerConfig};
use crate::cli::error::CliError;
use crate::cli::proxy_manager::{
    detect_proxy, detect_proxy_from_snapshot, generate_caddy_server_block,
    generate_nginx_server_block, ContainerRuntime, DockerCliRuntime, ProxyDetection,
};
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::AgentEnqueueRequest;
use crate::console::commands::cli::agent::{run_agent_command, AgentConfigureProxyCommand};
use crate::console::commands::CallableTrait;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyProviderKind {
    NginxProxyManager,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyProviderMetadata {
    pub kind: ProxyProviderKind,
    pub canonical_name: &'static str,
    pub service_catalog_name: &'static str,
    pub internal_api_url: &'static str,
}

impl ProxyProviderKind {
    pub fn from_alias(alias: &str) -> Option<Self> {
        match normalize_proxy_provider_alias(alias).as_str() {
            "npm" | "nginxproxymanager" => Some(Self::NginxProxyManager),
            _ => None,
        }
    }

    pub fn metadata(self) -> ProxyProviderMetadata {
        match self {
            Self::NginxProxyManager => ProxyProviderMetadata {
                kind: self,
                canonical_name: "nginx-proxy-manager",
                service_catalog_name: "nginx_proxy_manager",
                internal_api_url: "http://nginx-proxy-manager:81",
            },
        }
    }
}

fn normalize_proxy_provider_alias(alias: &str) -> String {
    alias
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

/// Parse SSL mode string to `SslMode` enum.
pub fn parse_ssl_mode(s: Option<&str>) -> SslMode {
    match s.map(|v| v.to_lowercase()).as_deref() {
        Some("auto") | Some("true") | Some("yes") | Some("on") | Some("1") => SslMode::Auto,
        Some("manual") => SslMode::Manual,
        Some("off") | Some("false") | Some("no") | Some("0") => SslMode::Off,
        _ => SslMode::Off,
    }
}

/// Build a `DomainConfig` from CLI arguments.
pub fn build_domain_config(
    domain: &str,
    upstream: Option<&str>,
    ssl: Option<&str>,
) -> DomainConfig {
    DomainConfig {
        domain: domain.to_string(),
        ssl: parse_ssl_mode(ssl),
        upstream: upstream.unwrap_or("http://app:8080").to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProxyConfigPersistence {
    config_path: PathBuf,
    backup_path: PathBuf,
    changed: bool,
}

fn upsert_proxy_domain_config(
    config: &mut StackerConfig,
    proxy_type: ProxyType,
    domain_config: DomainConfig,
) -> bool {
    let mut changed = false;

    if config.proxy.proxy_type != proxy_type {
        config.proxy.proxy_type = proxy_type;
        changed = true;
    }

    if let Some(existing) = config
        .proxy
        .domains
        .iter_mut()
        .find(|entry| entry.domain.eq_ignore_ascii_case(&domain_config.domain))
    {
        if existing.ssl != domain_config.ssl || existing.upstream != domain_config.upstream {
            existing.ssl = domain_config.ssl;
            existing.upstream = domain_config.upstream;
            changed = true;
        }
        return changed;
    }

    config.proxy.domains.push(domain_config);
    true
}

fn persist_proxy_config_to_stacker_yml(
    project_dir: &Path,
    proxy_type: ProxyType,
    domain_config: DomainConfig,
) -> Result<Option<ProxyConfigPersistence>, CliError> {
    let config_path = project_dir.join("stacker.yml");
    if !config_path.exists() {
        return Ok(None);
    }

    let backup_path = PathBuf::from(format!("{}.bak", config_path.display()));

    // Read raw YAML to surgically update proxy without losing other keys.
    let raw_text = std::fs::read_to_string(&config_path)?;
    let mut root: serde_yaml::Value = serde_yaml::from_str(&raw_text)
        .map_err(|e| CliError::ConfigValidation(format!("Invalid YAML: {}", e)))?;

    // Check if domain already exists and needs update
    let changed = if let Some(proxy) = root.get_mut("proxy") {
        // Ensure proxy type matches the requested provider even when the
        // proxy section already exists (e.g. `type: none` -> nginx-proxy-manager).
        let mut changed = false;
        if let Some(map) = proxy.as_mapping_mut() {
            let type_str = match proxy_type {
                ProxyType::Nginx => "nginx".to_string(),
                ProxyType::NginxProxyManager => "nginx-proxy-manager".to_string(),
                ProxyType::Traefik => "traefik".to_string(),
                ProxyType::Caddy => "caddy".to_string(),
                ProxyType::None => "none".to_string(),
            };
            let type_key = serde_yaml::Value::String("type".to_string());
            match map.get_mut(&type_key) {
                Some(current) if current.as_str() != Some(type_str.as_str()) => {
                    *current = serde_yaml::Value::String(type_str);
                    changed = true;
                }
                None => {
                    map.insert(type_key, serde_yaml::Value::String(type_str));
                    changed = true;
                }
                _ => {}
            }
        }
        if let Some(domains) = proxy.get_mut("domains") {
            if let Some(arr) = domains.as_sequence_mut() {
                let _domain_lower = domain_config.domain.to_lowercase();
                let existing = arr.iter_mut().find(|entry| {
                    entry
                        .get("domain")
                        .and_then(|v| v.as_str())
                        .map(|d| d.eq_ignore_ascii_case(&domain_config.domain))
                        .unwrap_or(false)
                });
                if let Some(entry) = existing {
                    let mut entry_changed = false;
                    if let Some(ssl) = entry.get_mut("ssl") {
                        let new_ssl = serde_yaml::to_value(&domain_config.ssl).unwrap_or_default();
                        if *ssl != new_ssl {
                            *ssl = new_ssl;
                            entry_changed = true;
                        }
                    }
                    if let Some(upstream) = entry.get_mut("upstream") {
                        let new_upstream =
                            serde_yaml::Value::String(domain_config.upstream.clone());
                        if *upstream != new_upstream {
                            *upstream = new_upstream;
                            entry_changed = true;
                        }
                    }
                    changed = changed || entry_changed;
                    changed
                } else {
                    // Add new domain entry
                    let new_entry = serde_yaml::to_value(&domain_config).unwrap_or_default();
                    arr.push(new_entry);
                    true
                }
            } else {
                // domains is not an array — replace it
                *domains = serde_yaml::Value::Sequence(vec![
                    serde_yaml::to_value(&domain_config).unwrap_or_default()
                ]);
                true
            }
        } else {
            // No domains key — add it
            if let Some(map) = proxy.as_mapping_mut() {
                let key = serde_yaml::Value::String("domains".to_string());
                let val = serde_yaml::Value::Sequence(vec![
                    serde_yaml::to_value(&domain_config).unwrap_or_default()
                ]);
                map.insert(key, val);
            }
            true
        }
    } else {
        // No proxy key — add it with type and domains
        if let Some(map) = root.as_mapping_mut() {
            let type_str = serde_yaml::Value::String(match proxy_type {
                ProxyType::Nginx => "nginx".to_string(),
                ProxyType::NginxProxyManager => "nginx-proxy-manager".to_string(),
                ProxyType::Traefik => "traefik".to_string(),
                ProxyType::Caddy => "caddy".to_string(),
                ProxyType::None => "none".to_string(),
            });
            let mut proxy_map = serde_yaml::Mapping::new();
            proxy_map.insert(serde_yaml::Value::String("type".to_string()), type_str);
            proxy_map.insert(
                serde_yaml::Value::String("domains".to_string()),
                serde_yaml::Value::Sequence(vec![
                    serde_yaml::to_value(&domain_config).unwrap_or_default()
                ]),
            );
            map.insert(
                serde_yaml::Value::String("proxy".to_string()),
                serde_yaml::Value::Mapping(proxy_map),
            );
        }
        true
    };

    if !changed {
        return Ok(Some(ProxyConfigPersistence {
            config_path,
            backup_path,
            changed,
        }));
    }

    // Rewrite only the `proxy:` block in place — everything else in the file
    // keeps its comments, quoting, inline lists and blank lines byte for byte
    // (BUGS.md #37: a whole-file YAML round-trip destroyed them all).
    let proxy_value = root.get("proxy").cloned().unwrap_or_default();
    let block = render_proxy_block(&proxy_value)?;
    let updated = replace_proxy_block(&raw_text, &block);
    std::fs::copy(&config_path, &backup_path)?;
    std::fs::write(&config_path, updated)?;

    Ok(Some(ProxyConfigPersistence {
        config_path,
        backup_path,
        changed,
    }))
}

/// Render the `proxy:` block as YAML nested one level under the key.
fn render_proxy_block(proxy: &serde_yaml::Value) -> Result<String, CliError> {
    let yaml = serde_yaml::to_string(proxy)
        .map_err(|e| CliError::ConfigValidation(format!("Failed to serialize config: {}", e)))?;
    let mut block = String::from("proxy:\n");
    for line in yaml.lines() {
        if line.is_empty() {
            block.push('\n');
        } else {
            block.push_str("  ");
            block.push_str(line);
            block.push('\n');
        }
    }
    Ok(block)
}

/// Replace (or append) the top-level `proxy:` block in raw `stacker.yml` text,
/// leaving every other byte untouched (BUGS.md #37).
fn replace_proxy_block(raw_text: &str, block: &str) -> String {
    let mut block = block.to_string();
    if !block.ends_with('\n') {
        block.push('\n');
    }

    // Byte offset of every line start (a trailing "\n" yields a final empty
    // line, which the bounds checks below tolerate).
    let mut line_starts = vec![0usize];
    for (idx, ch) in raw_text.char_indices() {
        if ch == '\n' {
            line_starts.push(idx + 1);
        }
    }
    let line_count = line_starts.len();
    let line_at = |i: usize| -> &str {
        let end = line_starts.get(i + 1).copied().unwrap_or(raw_text.len());
        &raw_text[line_starts[i]..end]
    };
    fn without_eol(line: &str) -> &str {
        line.trim_end_matches(['\n', '\r'])
    }

    // The block starts at the top-level `proxy:` key.
    let Some(start) = (0..line_count).find(|&i| without_eol(line_at(i)).starts_with("proxy:"))
    else {
        // No proxy block yet — append one after a blank separator line.
        let mut out = String::with_capacity(raw_text.len() + block.len() + 1);
        out.push_str(raw_text);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
        out.push_str(&block);
        return out;
    };

    // The block spans every following blank or indented line; top-level
    // comments are spanned too, so a comment in the middle of the block is
    // rewritten with it instead of cutting the block in half.
    let is_spanned = |line: &str| {
        let trimmed = without_eol(line);
        trimmed.is_empty() || line.starts_with(char::is_whitespace) || trimmed.starts_with('#')
    };
    let mut extent_end = start + 1;
    while extent_end < line_count && is_spanned(line_at(extent_end)) {
        extent_end += 1;
    }

    // Rewrite only up to the last indented line — blank lines and comments
    // after the block stay where they are.
    let mut content_end = start;
    for i in (start + 1..extent_end).rev() {
        let line = line_at(i);
        if line.starts_with(char::is_whitespace) && !without_eol(line).is_empty() {
            content_end = i;
            break;
        }
    }

    let tail_start = line_starts
        .get(content_end + 1)
        .copied()
        .unwrap_or(raw_text.len());
    let mut out = String::with_capacity(raw_text.len() + block.len());
    out.push_str(&raw_text[..line_starts[start]]);
    out.push_str(&block);
    out.push_str(&raw_text[tail_start..]);
    out
}

fn print_proxy_config_persistence(result: Option<&ProxyConfigPersistence>) {
    let Some(result) = result else {
        eprintln!("⚠ No stacker.yml found; proxy config was not persisted locally.");
        return;
    };

    if result.changed {
        eprintln!("✓ Updated proxy config in {}", result.config_path.display());
        eprintln!("  Backup written to {}", result.backup_path.display());
    } else {
        eprintln!(
            "✓ Proxy config already up to date in {}",
            result.config_path.display()
        );
    }
}

/// Render the snippet for a proxy type the remote agent cannot configure and
/// explain how to apply it (BUGS.md #37: `proxy add` forced
/// nginx-proxy-manager with no way to ask for anything else).
fn print_manual_proxy_config(
    proxy_type: ProxyType,
    domain_config: &DomainConfig,
    domain: &str,
) -> Result<(), CliError> {
    match proxy_type {
        ProxyType::Traefik => {
            eprintln!(
                "✓ Domain {} saved to stacker.yml; Traefik routing labels are generated \
                 automatically on the target service the next time you run `stacker deploy`.",
                domain
            );
        }
        ProxyType::Caddy => {
            println!("{}", generate_caddy_server_block(domain_config)?);
            eprintln!(
                "✓ Proxy config generated for {}; apply this Caddyfile snippet on the remote \
                 server (automatic remote configuration is only available for \
                 nginx-proxy-manager).",
                domain
            );
        }
        _ => {
            eprintln!(
                "✓ Domain {} saved to stacker.yml; Nginx Proxy Manager is deployed and \
                 configured on the next `stacker deploy`.",
                domain
            );
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyUpstreamTarget {
    pub app_code: String,
    pub port: u16,
}

pub fn parse_proxy_upstream(upstream: &str) -> Result<ProxyUpstreamTarget, CliError> {
    let upstream = upstream
        .strip_prefix("http://")
        .or_else(|| upstream.strip_prefix("https://"))
        .unwrap_or(upstream);

    if upstream.contains('/') {
        return Err(CliError::ConfigValidation(format!(
            "Invalid upstream '{}': paths are not supported; use host:port",
            upstream
        )));
    }

    let (host, port) = upstream.rsplit_once(':').ok_or_else(|| {
        CliError::ConfigValidation(format!(
            "Invalid upstream '{}': must match [http://]host:port format",
            upstream
        ))
    })?;

    if host.trim().is_empty() {
        return Err(CliError::ConfigValidation(format!(
            "Invalid upstream '{}': host is required",
            upstream
        )));
    }

    let port = port.parse::<u16>().map_err(|_| {
        CliError::ConfigValidation(format!(
            "Invalid upstream '{}': port must be between 1 and 65535",
            upstream
        ))
    })?;

    if port == 0 {
        return Err(CliError::ConfigValidation(format!(
            "Invalid upstream '{}': port must be between 1 and 65535",
            upstream
        )));
    }

    Ok(ProxyUpstreamTarget {
        app_code: host.to_string(),
        port,
    })
}

/// Run proxy detection using a `ContainerRuntime` (DIP).
pub fn run_detect(runtime: &dyn ContainerRuntime) -> Result<ProxyDetection, CliError> {
    detect_proxy(runtime)
}

/// Parse an explicit `--type` value into a `ProxyType` (BUGS.md #37: `proxy
/// add` hardcoded nginx-proxy-manager on the remote flow with no way to ask
/// for plain nginx).
pub fn parse_proxy_type(value: Option<&str>) -> Result<Option<ProxyType>, CliError> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    match value.to_ascii_lowercase().as_str() {
        "nginx" => Ok(Some(ProxyType::Nginx)),
        "nginx-proxy-manager" | "nginxproxymanager" | "npm" => {
            Ok(Some(ProxyType::NginxProxyManager))
        }
        "traefik" => Ok(Some(ProxyType::Traefik)),
        "caddy" => Ok(Some(ProxyType::Caddy)),
        other => Err(CliError::ConfigValidation(format!(
            "Invalid proxy type '{}': expected nginx, nginx-proxy-manager (npm), traefik or caddy",
            other
        ))),
    }
}

/// `stacker proxy add <domain> [--upstream <host:port>] [--ssl[=auto|manual|off]] [--type <TYPE>]`
///
/// Adds a reverse-proxy entry for the given domain.
pub struct ProxyAddCommand {
    pub domain: String,
    pub upstream: Option<String>,
    pub ssl: Option<String>,
    pub force: bool,
    pub json: bool,
    pub deployment: Option<String>,
    pub proxy_type: Option<String>,
}

impl ProxyAddCommand {
    pub fn new(
        domain: String,
        upstream: Option<String>,
        ssl: Option<String>,
        force: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            domain,
            upstream,
            ssl,
            force,
            json,
            deployment,
            proxy_type: None,
        }
    }

    pub fn with_proxy_type(mut self, proxy_type: Option<String>) -> Self {
        self.proxy_type = proxy_type;
        self
    }
}

impl CallableTrait for ProxyAddCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let requested_type = parse_proxy_type(self.proxy_type.as_deref())?;
        let project_dir = std::env::current_dir()?;
        let domain_config =
            build_domain_config(&self.domain, self.upstream.as_deref(), self.ssl.as_deref());
        let use_agent = self.deployment.is_some() || is_cloud_or_remote(&project_dir)?;
        if use_agent {
            // BUGS.md #42: `proxy.type: nginx` is a local-deploy type — refuse
            // on remote deployments before touching the config, instead of
            // silently substituting the managed Nginx Proxy Manager.
            if requested_type == Some(ProxyType::Nginx) {
                return Err(Box::new(CliError::ConfigValidation(
                    crate::cli::config_parser::PROXY_NGINX_REMOTE_UNSUPPORTED.to_string(),
                )));
            }

            // Persist to stacker.yml first — the local config update does not
            // depend on the remote agent or Vault. If the agent call later
            // fails, the user's config is still saved.
            let proxy_type = requested_type.unwrap_or(ProxyType::NginxProxyManager);
            let persistence = persist_proxy_config_to_stacker_yml(
                &project_dir,
                proxy_type,
                domain_config.clone(),
            )?;
            if !self.json {
                print_proxy_config_persistence(persistence.as_ref());
            }

            if proxy_type != ProxyType::NginxProxyManager {
                // The remote agent configures the managed Nginx Proxy Manager
                // only; caddy/traefik are recorded and rendered for manual
                // application instead of being silently flipped to NPM.
                print_manual_proxy_config(proxy_type, &domain_config, &self.domain)?;
                return Ok(());
            }

            let upstream = self.upstream.as_deref().unwrap_or("app:8080");
            let target = parse_proxy_upstream(upstream)?;
            let ssl_enabled = parse_ssl_mode(self.ssl.as_deref()) != SslMode::Off;
            let command = AgentConfigureProxyCommand::new(
                target.app_code,
                self.domain.clone(),
                target.port,
                ssl_enabled,
                !ssl_enabled,
                "create".to_string(),
                self.force,
                self.json,
                self.deployment.clone(),
            );
            if let Err(err) = command.call() {
                eprintln!(
                    "  ⚠ Proxy config saved locally, but remote NPM configuration failed: {}",
                    err
                );
                eprintln!(
                    "    The proxy will be configured on the remote server on the next deploy."
                );
            }
            return Ok(());
        }

        // An explicit --type nginx-proxy-manager on a local deployment: record
        // it and defer to deploy — nothing here can configure NPM by hand.
        if requested_type == Some(ProxyType::NginxProxyManager) {
            let persistence = persist_proxy_config_to_stacker_yml(
                &project_dir,
                ProxyType::NginxProxyManager,
                domain_config,
            )?;
            if !self.json {
                print_proxy_config_persistence(persistence.as_ref());
            }
            eprintln!(
                "✓ Domain {} saved to stacker.yml; Nginx Proxy Manager is deployed and configured \
                 on the next `stacker deploy`.",
                self.domain
            );
            return Ok(());
        }

        // Keep an already-configured Caddy/Traefik proxy as-is; default to
        // nginx otherwise, matching prior behavior for everyone who hasn't
        // opted into either. `--type` overrides the configured type.
        let existing_proxy_type = requested_type.unwrap_or_else(|| {
            StackerConfig::from_file(&project_dir.join("stacker.yml"))
                .map(|config| config.proxy.proxy_type)
                .unwrap_or(ProxyType::Nginx)
        });

        if existing_proxy_type == ProxyType::Traefik {
            // Traefik routes via labels baked directly into the generated
            // docker-compose.yml (see `ComposeDefinition::try_from`'s
            // domain-driven label injection) — there's no separate config
            // file to print and hand-apply like nginx/Caddy.
            let persistence = persist_proxy_config_to_stacker_yml(
                &project_dir,
                ProxyType::Traefik,
                domain_config,
            )?;
            if !self.json {
                print_proxy_config_persistence(persistence.as_ref());
            }
            eprintln!(
                "✓ Domain {} saved to stacker.yml; Traefik routing labels are generated \
                 automatically on the target service the next time you run `stacker deploy`.",
                self.domain
            );
            return Ok(());
        }

        let (block, proxy_type, kind_label) = if existing_proxy_type == ProxyType::Caddy {
            (
                generate_caddy_server_block(&domain_config)?,
                ProxyType::Caddy,
                "Caddyfile",
            )
        } else {
            (
                generate_nginx_server_block(&domain_config)?,
                ProxyType::Nginx,
                "nginx",
            )
        };
        let persistence =
            persist_proxy_config_to_stacker_yml(&project_dir, proxy_type, domain_config)?;
        println!("{}", block);
        if !self.json {
            print_proxy_config_persistence(persistence.as_ref());
        }
        eprintln!(
            "✓ Proxy config generated for {}; apply this {} snippet to configure a local proxy",
            self.domain, kind_label
        );
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

/// Check whether the current project is deployed to cloud/remote (agent path)
/// or local (direct docker), via the shared deployment-context resolver.
fn is_cloud_or_remote(project_dir: &std::path::Path) -> Result<bool, CliError> {
    Ok(crate::cli::deployment_context::resolve_deploy_placement(project_dir)?.is_remote())
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
        let use_remote = self.deployment.is_some() || is_cloud_or_remote(&project_dir)?;

        if use_remote {
            let ctx = CliRuntime::new("proxy detect")?;
            let hash =
                crate::cli::deployment_context::resolve_deployment_hash(&self.deployment, &ctx)?;

            // Use a live list_containers command — the snapshot's containers
            // field is not populated for cloud deployments.
            const DETECT_TIMEOUT: u64 = 30;
            let params = crate::forms::status_panel::ListContainersCommandRequest {
                include_health: true,
                include_logs: false,
                log_lines: 0,
            };
            let request = AgentEnqueueRequest::new(&hash, "list_containers")
                .with_parameters(&params)
                .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;
            let info = run_agent_command(&ctx, &request, "Scanning containers", DETECT_TIMEOUT)?;
            let containers = info
                .result
                .as_ref()
                .and_then(|r| r.get("containers").and_then(|v| v.as_array()))
                .cloned()
                .unwrap_or_default();
            let fake_snapshot = serde_json::json!({ "containers": containers });
            let detection = detect_proxy_from_snapshot(&fake_snapshot);
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
    use crate::cli::config_parser::{ConfigBuilder, ProxyConfig};
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
    fn proxy_provider_aliases_resolve_to_nginx_proxy_manager() {
        for alias in [
            "npm",
            "nginx-proxy-manager",
            "nginx_proxy_manager",
            "Nginx Proxy Manager",
        ] {
            assert_eq!(
                ProxyProviderKind::from_alias(alias),
                Some(ProxyProviderKind::NginxProxyManager)
            );
        }
        assert_eq!(ProxyProviderKind::from_alias("traefik"), None);
    }

    #[test]
    fn nginx_proxy_manager_metadata_uses_stack_service_defaults() {
        let metadata = ProxyProviderKind::NginxProxyManager.metadata();

        assert_eq!(metadata.service_catalog_name, "nginx_proxy_manager");
        assert_eq!(metadata.internal_api_url, "http://nginx-proxy-manager:81");
        assert_eq!(metadata.canonical_name, "nginx-proxy-manager");
    }

    #[test]
    fn test_parse_ssl_mode_auto() {
        assert_eq!(parse_ssl_mode(Some("auto")), SslMode::Auto);
        assert_eq!(parse_ssl_mode(Some("AUTO")), SslMode::Auto);
        assert_eq!(parse_ssl_mode(Some("true")), SslMode::Auto);
    }

    #[test]
    fn test_parse_ssl_mode_defaults_to_off() {
        assert_eq!(parse_ssl_mode(None), SslMode::Off);
        assert_eq!(parse_ssl_mode(Some("unknown")), SslMode::Off);
        assert_eq!(parse_ssl_mode(Some("false")), SslMode::Off);
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
    fn upsert_proxy_domain_config_sets_type_and_adds_domain() {
        let mut config = ConfigBuilder::new().name("demo").build().unwrap();
        let changed = upsert_proxy_domain_config(
            &mut config,
            ProxyType::NginxProxyManager,
            build_domain_config("example.com", Some("app:3000"), Some("auto")),
        );

        assert!(changed);
        assert_eq!(config.proxy.proxy_type, ProxyType::NginxProxyManager);
        assert_eq!(config.proxy.domains.len(), 1);
        assert_eq!(config.proxy.domains[0].domain, "example.com");
        assert_eq!(config.proxy.domains[0].ssl, SslMode::Auto);
        assert_eq!(config.proxy.domains[0].upstream, "app:3000");
    }

    #[test]
    fn upsert_proxy_domain_config_updates_existing_domain_without_duplicate() {
        let mut config = ConfigBuilder::new()
            .name("demo")
            .proxy(ProxyConfig {
                proxy_type: ProxyType::None,
                auto_detect: false,
                domains: vec![build_domain_config(
                    "Example.com",
                    Some("app:3000"),
                    Some("off"),
                )],
                config: None,
            })
            .build()
            .unwrap();

        let changed = upsert_proxy_domain_config(
            &mut config,
            ProxyType::NginxProxyManager,
            build_domain_config("example.com", Some("web:8080"), Some("auto")),
        );

        assert!(changed);
        assert_eq!(config.proxy.proxy_type, ProxyType::NginxProxyManager);
        assert_eq!(config.proxy.domains.len(), 1);
        assert_eq!(config.proxy.domains[0].domain, "Example.com");
        assert_eq!(config.proxy.domains[0].ssl, SslMode::Auto);
        assert_eq!(config.proxy.domains[0].upstream, "web:8080");
    }

    #[test]
    fn persist_proxy_config_to_stacker_yml_writes_backup_and_preserves_env_placeholders() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("stacker.yml");
        std::fs::write(
            &config_path,
            "name: demo\napp:\n  type: node\n  image: ${APP_IMAGE}\nproxy:\n  type: none\n  domains: []\n",
        )
        .unwrap();

        let result = persist_proxy_config_to_stacker_yml(
            dir.path(),
            ProxyType::NginxProxyManager,
            build_domain_config(
                "status.example.com",
                Some("status-panel-web:3000"),
                Some("auto"),
            ),
        )
        .unwrap()
        .expect("stacker.yml exists");

        assert!(result.changed);
        assert!(result.backup_path.exists());

        let written = std::fs::read_to_string(&config_path).unwrap();
        assert!(written.contains("${APP_IMAGE}"));

        let config = StackerConfig::from_file_raw(&config_path).unwrap();
        assert_eq!(config.proxy.proxy_type, ProxyType::NginxProxyManager);
        assert_eq!(config.proxy.domains.len(), 1);
        assert_eq!(config.proxy.domains[0].domain, "status.example.com");
        assert_eq!(config.proxy.domains[0].upstream, "status-panel-web:3000");
        assert_eq!(config.proxy.domains[0].ssl, SslMode::Auto);
    }

    #[test]
    fn given_stacker_proxy_add_when_config_is_persisted_then_stacker_yml_reflects_proxy_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("stacker.yml");
        std::fs::write(
            &config_path,
            r#"
name: web
services:
  - name: status-panel-web
    image: trydirect/status-panel-web:0.1.0
proxy:
  type: none
  auto_detect: false
  domains: []
"#,
        )
        .unwrap();

        let result = persist_proxy_config_to_stacker_yml(
            dir.path(),
            ProxyType::NginxProxyManager,
            build_domain_config(
                "status.stacker.my",
                Some("status-panel-web:3000"),
                Some("auto"),
            ),
        )
        .unwrap()
        .expect("stacker.yml exists");

        assert!(result.changed);
        assert!(result.backup_path.exists());

        let config = StackerConfig::from_file_raw(&config_path).unwrap();
        assert_eq!(config.proxy.proxy_type, ProxyType::NginxProxyManager);
        assert_eq!(config.proxy.domains.len(), 1);
        assert_eq!(config.proxy.domains[0].domain, "status.stacker.my");
        assert_eq!(config.proxy.domains[0].ssl, SslMode::Auto);
        assert_eq!(config.proxy.domains[0].upstream, "status-panel-web:3000");
        assert!(config
            .services
            .iter()
            .all(|service| service.name != "nginx_proxy_manager"));
    }

    // BUGS.md #37: one `proxy add` round-tripped the whole stacker.yml through
    // a YAML serializer — every comment vanished, quoting was normalised,
    // inline lists expanded and blank lines dropped. The edit must rewrite only
    // the `proxy:` block and leave every other byte untouched.
    #[test]
    fn persist_proxy_config_to_stacker_yml_preserves_everything_outside_the_proxy_block() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("stacker.yml");
        let before = concat!(
            "# n8n template — documented example\n",
            "name: n8n\n",
            "version: \"2.0.0\"\n",
            "\n",
            "app:\n",
            "  type: node\n",
            "  ports: [\"5678:5678\"]\n",
            "  environment:\n",
            "    # ${BASE_PATH}/data, see the README\n",
            "    EMAIL_REGEX: \"^[^@\\\\s\\\"]+@example\\\\.com$\"\n",
            "\n",
            "proxy:\n",
            "  type: none\n",
            "  domains: []\n",
            "\n",
            "# deploy section follows\n",
            "deploy:\n",
            "  target: server\n",
        );
        std::fs::write(&config_path, before).unwrap();

        let result = persist_proxy_config_to_stacker_yml(
            dir.path(),
            ProxyType::NginxProxyManager,
            build_domain_config("n8n.example.com", Some("http://n8n:5678"), Some("off")),
        )
        .unwrap()
        .expect("stacker.yml exists");

        assert!(result.changed);
        let after = std::fs::read_to_string(&config_path).unwrap();

        // Everything before the proxy block is byte-identical.
        let proxy_start = before.find("proxy:").unwrap();
        assert!(after.starts_with(&before[..proxy_start]));
        // Comments survive.
        assert!(after.contains("# n8n template — documented example\n"));
        assert!(after.contains("    # ${BASE_PATH}/data, see the README\n"));
        assert!(after.contains("# deploy section follows\n"));
        // Quoting and inline lists survive.
        assert!(after.contains("version: \"2.0.0\"\n"));
        assert!(after.contains("  ports: [\"5678:5678\"]\n"));
        assert!(after.contains("    EMAIL_REGEX: \"^[^@\\\\s\\\"]+@example\\\\.com$\"\n"));
        // Blank lines survive.
        assert!(after.contains("version: \"2.0.0\"\n\napp:\n"));
        assert!(after.contains("\n\n# deploy section follows\n"));
        // The proxy entry itself landed.
        let config = StackerConfig::from_file_raw(&config_path).unwrap();
        assert_eq!(config.proxy.proxy_type, ProxyType::NginxProxyManager);
        assert_eq!(config.proxy.domains.len(), 1);
        assert_eq!(config.proxy.domains[0].domain, "n8n.example.com");
    }

    #[test]
    fn persist_proxy_config_to_stacker_yml_appends_proxy_block_when_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("stacker.yml");
        let before = "# a commented template\nname: demo\n\napp:\n  type: node\n";
        std::fs::write(&config_path, before).unwrap();

        let result = persist_proxy_config_to_stacker_yml(
            dir.path(),
            ProxyType::Nginx,
            build_domain_config("demo.example.com", Some("http://app:8080"), Some("off")),
        )
        .unwrap()
        .expect("stacker.yml exists");

        assert!(result.changed);
        let after = std::fs::read_to_string(&config_path).unwrap();
        assert!(after.starts_with(before));
        assert!(after.contains("# a commented template\n"));
        assert!(after.contains("\nproxy:\n"));

        let config = StackerConfig::from_file_raw(&config_path).unwrap();
        assert_eq!(config.proxy.domains.len(), 1);
        assert_eq!(config.proxy.domains[0].domain, "demo.example.com");
    }

    #[test]
    fn test_parse_proxy_type_accepts_aliases_and_rejects_unknown() {
        assert_eq!(parse_proxy_type(None).unwrap(), None);
        assert_eq!(
            parse_proxy_type(Some("nginx")).unwrap(),
            Some(ProxyType::Nginx)
        );
        assert_eq!(
            parse_proxy_type(Some("npm")).unwrap(),
            Some(ProxyType::NginxProxyManager)
        );
        assert_eq!(
            parse_proxy_type(Some("nginx-proxy-manager")).unwrap(),
            Some(ProxyType::NginxProxyManager)
        );
        assert_eq!(
            parse_proxy_type(Some("traefik")).unwrap(),
            Some(ProxyType::Traefik)
        );
        assert_eq!(
            parse_proxy_type(Some("caddy")).unwrap(),
            Some(ProxyType::Caddy)
        );
        assert!(parse_proxy_type(Some("haproxy")).is_err());
    }

    #[test]
    fn test_parse_proxy_upstream_strips_scheme() {
        let target = parse_proxy_upstream("http://coolify:80").unwrap();
        assert_eq!(target.app_code, "coolify");
        assert_eq!(target.port, 80);
    }

    #[test]
    fn test_parse_proxy_upstream_rejects_paths() {
        let err = parse_proxy_upstream("http://coolify:80/admin").unwrap_err();
        assert!(err.to_string().contains("paths are not supported"));
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
