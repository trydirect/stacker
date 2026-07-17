use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cli::config_parser::{ProxyConfig, ProxyType, ServiceDefinition, StackerConfig};
use crate::cli::error::CliError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposeServiceSyncResult {
    pub compose_path: Option<PathBuf>,
    pub backup_path: Option<PathBuf>,
    pub updated_services: Vec<String>,
}

/// Extract the service name from an upstream string like `svc:3000` or `http://svc:3000`.
pub fn upstream_service_name(upstream: &str) -> Option<String> {
    let s = upstream
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = s.split('/').next()?;
    let name = host.split(':').next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Inject `default_network` into `service_name` inside `compose_doc` when the service is
/// listed as an NginxProxyManager upstream. Declares the network as `external: true` at
/// the top level. Returns `true` if the document was modified.
pub fn inject_npm_proxy_network(
    compose_doc: &mut serde_yaml::Value,
    service_name: &str,
    proxy: &ProxyConfig,
) -> bool {
    if proxy.proxy_type != ProxyType::NginxProxyManager {
        return false;
    }
    let is_proxied = proxy.domains.iter().any(|d| {
        upstream_service_name(&d.upstream)
            .map(|n| n == service_name)
            .unwrap_or(false)
    });
    if !is_proxied {
        return false;
    }
    inject_external_network(compose_doc, service_name, "default_network")
}

/// Attach every service in `compose_doc` to a shared `external: true` `network`
/// and declare it at the top level. Idempotent — services already on the network
/// are left unchanged. Returns `true` if the document was modified.
///
/// Used so status-panel/agent deploys can reach the project's containers: the
/// agent lives on `default_network`, and project containers must share it. The
/// CLI-generated compose already joins `default_network`; this also covers
/// user-supplied composes that define their own network.
pub fn inject_shared_network_all_services(
    compose_doc: &mut serde_yaml::Value,
    network: &str,
) -> bool {
    let service_names: Vec<String> = compose_doc
        .get("services")
        .and_then(serde_yaml::Value::as_mapping)
        .map(|services| {
            services
                .keys()
                .filter_map(|key| key.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut changed = false;
    for service_name in service_names {
        if inject_external_network(compose_doc, &service_name, network) {
            changed = true;
        }
    }
    changed
}

fn inject_external_network(
    compose_doc: &mut serde_yaml::Value,
    service_name: &str,
    network: &str,
) -> bool {
    let mut changed = false;
    let network_val = serde_yaml::Value::String(network.to_string());

    if let Some(svc) = compose_doc
        .get_mut("services")
        .and_then(|s| s.get_mut(service_name))
        .and_then(serde_yaml::Value::as_mapping_mut)
    {
        let networks_key = serde_yaml::Value::String("networks".to_string());
        match svc.get_mut(&networks_key) {
            Some(serde_yaml::Value::Sequence(seq)) => {
                if !seq.contains(&network_val) {
                    seq.push(network_val);
                    changed = true;
                }
            }
            None => {
                svc.insert(networks_key, serde_yaml::Value::Sequence(vec![network_val]));
                changed = true;
            }
            _ => {}
        }
    }

    if changed {
        upsert_external_network(compose_doc, network);
    }
    changed
}

fn upsert_external_network(compose_doc: &mut serde_yaml::Value, network: &str) {
    let Some(root) = compose_doc.as_mapping_mut() else {
        return;
    };
    let networks_key = serde_yaml::Value::String("networks".to_string());
    if !root.contains_key(&networks_key) {
        root.insert(
            networks_key.clone(),
            serde_yaml::Value::Mapping(Default::default()),
        );
    }
    if let Some(top_networks) = root
        .get_mut(&networks_key)
        .and_then(serde_yaml::Value::as_mapping_mut)
    {
        let net_key = serde_yaml::Value::String(network.to_string());
        if !top_networks.contains_key(&net_key) {
            let mut net_config = serde_yaml::Mapping::new();
            net_config.insert(
                serde_yaml::Value::String("external".to_string()),
                serde_yaml::Value::Bool(true),
            );
            top_networks.insert(net_key, serde_yaml::Value::Mapping(net_config));
        }
    }
}

pub fn sync_configured_compose_services(
    project_dir: &Path,
    config: &StackerConfig,
    service_names: &[String],
) -> Result<ComposeServiceSyncResult, CliError> {
    let Some(compose_file) = config.deploy.compose_file.as_ref() else {
        return Ok(ComposeServiceSyncResult::default());
    };
    if service_names.is_empty() {
        return Ok(ComposeServiceSyncResult {
            compose_path: Some(resolve_path(project_dir, compose_file)),
            ..Default::default()
        });
    }

    let compose_path = resolve_path(project_dir, compose_file);
    if !compose_path.exists() {
        return Err(CliError::ConfigValidation(format!(
            "Configured compose file does not exist: {}",
            compose_path.display()
        )));
    }

    let original = std::fs::read_to_string(&compose_path)?;
    let mut compose_doc: serde_yaml::Value = serde_yaml::from_str(&original)?;
    let project_networks = project_service_networks(&compose_doc);
    let mut updated_services = Vec::new();

    for service_name in service_names {
        let service = config
            .services
            .iter()
            .find(|service| service.name == *service_name)
            .ok_or_else(|| {
                CliError::ConfigValidation(format!(
                    "Service '{}' was not found in stacker.yml",
                    service_name
                ))
            })?;

        let mut svc_networks = project_networks.clone();
        if config.proxy.proxy_type == ProxyType::NginxProxyManager
            && !svc_networks.contains(&"default_network".to_string())
            && config.proxy.domains.iter().any(|d| {
                upstream_service_name(&d.upstream)
                    .map(|n| n == *service_name)
                    .unwrap_or(false)
            })
        {
            svc_networks.push("default_network".to_string());
            upsert_external_network(&mut compose_doc, "default_network");
        }

        upsert_compose_service(&mut compose_doc, service, &svc_networks)?;
        updated_services.push(service.name.clone());
    }

    let updated = serde_yaml::to_string(&compose_doc)
        .map_err(|err| CliError::ConfigValidation(format!("failed to serialize compose: {err}")))?;
    if updated == original {
        return Ok(ComposeServiceSyncResult {
            compose_path: Some(compose_path),
            backup_path: None,
            updated_services: Vec::new(),
        });
    }

    let backup_path = backup_path(&compose_path);
    std::fs::copy(&compose_path, &backup_path)?;
    std::fs::write(&compose_path, updated)?;

    Ok(ComposeServiceSyncResult {
        compose_path: Some(compose_path),
        backup_path: Some(backup_path),
        updated_services,
    })
}

fn resolve_path(project_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    }
}

fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.bak", path.to_string_lossy()))
}

fn upsert_compose_service(
    compose_doc: &mut serde_yaml::Value,
    service: &ServiceDefinition,
    project_networks: &[String],
) -> Result<(), CliError> {
    let services_key = serde_yaml::Value::String("services".to_string());
    let root = compose_doc.as_mapping_mut().ok_or_else(|| {
        CliError::ConfigValidation("docker compose file must be a YAML mapping".to_string())
    })?;
    if !root.contains_key(&services_key) {
        root.insert(
            services_key.clone(),
            serde_yaml::Value::Mapping(Default::default()),
        );
    }
    let services = root
        .get_mut(&services_key)
        .and_then(serde_yaml::Value::as_mapping_mut)
        .ok_or_else(|| {
            CliError::ConfigValidation("docker compose file services must be a mapping".to_string())
        })?;

    services.insert(
        serde_yaml::Value::String(service.name.clone()),
        service_to_compose_value(service, project_networks),
    );
    upsert_named_volumes(root, &service.volumes);
    Ok(())
}

fn service_to_compose_value(
    service: &ServiceDefinition,
    project_networks: &[String],
) -> serde_yaml::Value {
    let mut map = serde_yaml::Mapping::new();
    map.insert(
        serde_yaml::Value::String("image".to_string()),
        serde_yaml::Value::String(service.image.clone()),
    );
    insert_string_sequence(&mut map, "ports", &service.ports);
    insert_environment(&mut map, &service.environment);
    if let Some(ref cmd) = service.command {
        map.insert(
            serde_yaml::Value::String("command".to_string()),
            serde_yaml::Value::String(cmd.clone()),
        );
    }
    if let Some(ref hc) = service.healthcheck {
        let mut hc_map = serde_yaml::Mapping::new();
        hc_map.insert(
            serde_yaml::Value::String("test".to_string()),
            serde_yaml::Value::String(hc.test.clone()),
        );
        hc_map.insert(
            serde_yaml::Value::String("interval".to_string()),
            serde_yaml::Value::String(hc.interval.clone()),
        );
        hc_map.insert(
            serde_yaml::Value::String("timeout".to_string()),
            serde_yaml::Value::String(hc.timeout.clone()),
        );
        hc_map.insert(
            serde_yaml::Value::String("retries".to_string()),
            serde_yaml::Value::Number(hc.retries.into()),
        );
        map.insert(
            serde_yaml::Value::String("healthcheck".to_string()),
            serde_yaml::Value::Mapping(hc_map),
        );
    }
    insert_string_sequence(&mut map, "volumes", &service.volumes);
    insert_string_sequence(&mut map, "depends_on", &service.depends_on);
    if !project_networks.is_empty() {
        insert_string_sequence(&mut map, "networks", project_networks);
    }
    map.insert(
        serde_yaml::Value::String("restart".to_string()),
        serde_yaml::Value::String("unless-stopped".to_string()),
    );
    serde_yaml::Value::Mapping(map)
}

fn insert_string_sequence(map: &mut serde_yaml::Mapping, key: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    map.insert(
        serde_yaml::Value::String(key.to_string()),
        serde_yaml::Value::Sequence(
            values
                .iter()
                .map(|value| serde_yaml::Value::String(value.clone()))
                .collect(),
        ),
    );
}

fn insert_environment(
    map: &mut serde_yaml::Mapping,
    environment: &std::collections::HashMap<String, String>,
) {
    if environment.is_empty() {
        return;
    }
    let sorted: BTreeMap<_, _> = environment.iter().collect();
    let mut env_map = serde_yaml::Mapping::new();
    for (key, value) in sorted {
        env_map.insert(
            serde_yaml::Value::String(key.clone()),
            serde_yaml::Value::String(value.clone()),
        );
    }
    map.insert(
        serde_yaml::Value::String("environment".to_string()),
        serde_yaml::Value::Mapping(env_map),
    );
}

fn upsert_named_volumes(root: &mut serde_yaml::Mapping, volumes: &[String]) {
    let named_volumes: Vec<String> = volumes
        .iter()
        .filter_map(|volume| named_volume_source(volume))
        .collect();
    if named_volumes.is_empty() {
        return;
    }

    let volumes_key = serde_yaml::Value::String("volumes".to_string());
    if !root.contains_key(&volumes_key) {
        root.insert(
            volumes_key.clone(),
            serde_yaml::Value::Mapping(Default::default()),
        );
    }
    let Some(volume_map) = root
        .get_mut(&volumes_key)
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        return;
    };
    for volume in named_volumes {
        let key = serde_yaml::Value::String(volume.clone());
        if volume_map.contains_key(&key) {
            continue;
        }
        let mut value = serde_yaml::Mapping::new();
        value.insert(
            serde_yaml::Value::String("name".to_string()),
            serde_yaml::Value::String(volume),
        );
        volume_map.insert(key, serde_yaml::Value::Mapping(value));
    }
}

fn named_volume_source(volume: &str) -> Option<String> {
    let (source, _) = volume.split_once(':')?;
    if source.starts_with('.') || source.starts_with('/') || source.starts_with('$') {
        return None;
    }
    Some(source.to_string())
}

fn project_service_networks(project_doc: &serde_yaml::Value) -> Vec<String> {
    let Some(project_services) = project_doc
        .as_mapping()
        .and_then(|root| root.get(serde_yaml::Value::String("services".to_string())))
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return Vec::new();
    };

    let mut networks = Vec::new();
    for service in project_services.values() {
        let Some(networks_value) = service
            .as_mapping()
            .and_then(|service| service.get(serde_yaml::Value::String("networks".to_string())))
        else {
            continue;
        };
        collect_network_names(networks_value, &mut networks);
    }
    networks
}

fn collect_network_names(value: &serde_yaml::Value, networks: &mut Vec<String>) {
    match value {
        serde_yaml::Value::String(name) => push_unique_network(networks, name),
        serde_yaml::Value::Sequence(items) => {
            for item in items {
                if let Some(name) = item.as_str() {
                    push_unique_network(networks, name);
                }
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for key in map.keys() {
                if let Some(name) = key.as_str() {
                    push_unique_network(networks, name);
                }
            }
        }
        _ => {}
    }
}

fn push_unique_network(networks: &mut Vec<String>, name: &str) {
    if !networks.iter().any(|existing| existing == name) {
        networks.push(name.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::config_parser::{
        AppSource, DeployConfig, DomainConfig, ProjectConfig, SslMode,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn inject_shared_network_all_services_is_idempotent_and_covers_all() {
        let mut doc: serde_yaml::Value = serde_yaml::from_str(
            "services:\n  app:\n    image: a\n    networks: [default_network]\n  db:\n    image: b\n",
        )
        .unwrap();

        // First pass: only `db` is missing the network → changed.
        assert!(inject_shared_network_all_services(
            &mut doc,
            "default_network"
        ));
        for svc in ["app", "db"] {
            let nets = doc["services"][svc]["networks"].as_sequence().unwrap();
            assert_eq!(
                nets.iter()
                    .filter(|n| n.as_str() == Some("default_network"))
                    .count(),
                1,
                "service {svc} should have default_network exactly once"
            );
        }
        assert_eq!(doc["networks"]["default_network"]["external"], true);

        // Second pass: everything already present → no change (idempotent).
        assert!(!inject_shared_network_all_services(
            &mut doc,
            "default_network"
        ));
    }

    // ── inject_npm_proxy_network unit tests ──────────────────────────────────

    fn npm_proxy_config(upstream: &str) -> ProxyConfig {
        ProxyConfig {
            proxy_type: ProxyType::NginxProxyManager,
            auto_detect: false,
            domains: vec![DomainConfig {
                domain: "app.example.com".into(),
                ssl: SslMode::Auto,
                upstream: upstream.to_string(),
            }],
            config: None,
        }
    }

    fn compose_doc_with_service(service: &str) -> serde_yaml::Value {
        serde_yaml::from_str(&format!(
            "services:\n  {service}:\n    image: myapp:latest\n"
        ))
        .unwrap()
    }

    #[test]
    fn inject_npm_proxy_network_adds_to_proxied_service() {
        let mut doc = compose_doc_with_service("web");
        let changed = inject_npm_proxy_network(&mut doc, "web", &npm_proxy_config("web:3000"));
        assert!(changed);
        let networks = doc["services"]["web"]["networks"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(networks.contains(&"default_network"));
        // top-level declares it external
        assert_eq!(
            doc["networks"]["default_network"]["external"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn inject_npm_proxy_network_returns_false_for_non_proxied_service() {
        let mut doc = compose_doc_with_service("smtp");
        let changed = inject_npm_proxy_network(&mut doc, "smtp", &npm_proxy_config("web:3000"));
        assert!(!changed);
        assert!(doc["services"]["smtp"].get("networks").is_none());
    }

    #[test]
    fn inject_npm_proxy_network_returns_false_for_non_npm_proxy() {
        let mut doc = compose_doc_with_service("web");
        let proxy = ProxyConfig {
            proxy_type: ProxyType::Traefik,
            auto_detect: false,
            domains: vec![DomainConfig {
                domain: "app.example.com".into(),
                ssl: SslMode::Auto,
                upstream: "web:3000".into(),
            }],
            config: None,
        };
        let changed = inject_npm_proxy_network(&mut doc, "web", &proxy);
        assert!(!changed);
    }

    #[test]
    fn inject_npm_proxy_network_is_idempotent() {
        let mut doc: serde_yaml::Value = serde_yaml::from_str(
            "services:\n  web:\n    image: myapp:latest\n    networks:\n      - default_network\n",
        )
        .unwrap();
        let changed = inject_npm_proxy_network(&mut doc, "web", &npm_proxy_config("web:3000"));
        assert!(!changed, "already has default_network — should be a no-op");
        let seq = doc["services"]["web"]["networks"].as_sequence().unwrap();
        let count = seq
            .iter()
            .filter(|v| v.as_str() == Some("default_network"))
            .count();
        assert_eq!(count, 1, "no duplicate entries");
    }

    #[test]
    fn inject_npm_proxy_network_parses_http_prefix_upstream() {
        let proxy = ProxyConfig {
            proxy_type: ProxyType::NginxProxyManager,
            auto_detect: false,
            domains: vec![DomainConfig {
                domain: "app.example.com".into(),
                ssl: SslMode::Off,
                upstream: "http://api:8080".into(),
            }],
            config: None,
        };
        let mut doc = compose_doc_with_service("api");
        let changed = inject_npm_proxy_network(&mut doc, "api", &proxy);
        assert!(changed);
        let networks = doc["services"]["api"]["networks"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(networks.contains(&"default_network"));
    }

    // ── sync_configured_compose_services proxy-inject tests ──────────────────

    fn npm_stacker_config(_dir: &std::path::Path, service_name: &str) -> StackerConfig {
        StackerConfig {
            project: ProjectConfig::default(),
            app: AppSource::default(),
            deploy: DeployConfig {
                compose_file: Some(PathBuf::from("docker-compose.yml")),
                ..Default::default()
            },
            proxy: ProxyConfig {
                proxy_type: ProxyType::NginxProxyManager,
                auto_detect: false,
                domains: vec![DomainConfig {
                    domain: "app.example.com".into(),
                    ssl: SslMode::Auto,
                    upstream: format!("{service_name}:3000"),
                }],
                config: None,
            },
            services: vec![ServiceDefinition {
                name: service_name.to_string(),
                image: "myapp:latest".to_string(),
                ports: vec!["3000:3000".to_string()],
                environment: HashMap::new(),
                volumes: vec![],
                depends_on: vec![],
                command: None,
                healthcheck: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn sync_injects_default_network_for_npm_proxied_service() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("docker-compose.yml"),
            "services:\n  existing:\n    image: nginx:latest\n",
        )
        .unwrap();

        let config = npm_stacker_config(dir.path(), "api");
        let result =
            sync_configured_compose_services(dir.path(), &config, &["api".to_string()]).unwrap();

        assert_eq!(result.updated_services, vec!["api"]);
        let updated = std::fs::read_to_string(dir.path().join("docker-compose.yml")).unwrap();
        assert!(
            updated.contains("default_network"),
            "proxied service should have default_network injected:\n{updated}"
        );
        assert!(
            updated.contains("external: true") || updated.contains("external: 'true'"),
            "default_network should be declared external:\n{updated}"
        );
    }

    #[test]
    fn sync_does_not_inject_default_network_for_non_proxied_service() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("docker-compose.yml"),
            "services:\n  existing:\n    image: nginx:latest\n",
        )
        .unwrap();

        // proxy points to "api" but we are syncing "smtp"
        let mut config = npm_stacker_config(dir.path(), "api");
        config.services = vec![ServiceDefinition {
            name: "smtp".to_string(),
            image: "trydirect/smtp".to_string(),
            ports: vec![],
            environment: HashMap::new(),
            volumes: vec![],
            depends_on: vec![],
            command: None,
            healthcheck: None,
        }];

        let result =
            sync_configured_compose_services(dir.path(), &config, &["smtp".to_string()]).unwrap();

        assert_eq!(result.updated_services, vec!["smtp"]);
        let updated = std::fs::read_to_string(dir.path().join("docker-compose.yml")).unwrap();
        let smtp_section_start = updated.find("smtp:").unwrap();
        let smtp_section = &updated[smtp_section_start..];
        // "smtp" block should not list default_network
        let next_service = smtp_section[5..].find('\n').map(|i| &smtp_section[..i + 5]);
        let _ = next_service; // just ensure smtp block doesn't have it
        assert!(
            !smtp_section
                .lines()
                .take(10)
                .any(|l| l.contains("default_network")),
            "non-proxied service should not get default_network:\n{updated}"
        );
    }

    #[test]
    fn sync_configured_compose_services_upserts_service_networks_and_volumes() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("docker-compose.yml"),
            r#"
version: '3.8'
networks:
  default_network:
    external: true
    name: default_network
services:
  status-panel-web:
    image: trydirect/status-panel-web:latest
    networks:
      - default_network
volumes:
  npm_data:
    name: npm_data
"#,
        )
        .unwrap();

        let config = StackerConfig {
            project: ProjectConfig::default(),
            app: AppSource::default(),
            deploy: DeployConfig {
                compose_file: Some(PathBuf::from("docker-compose.yml")),
                ..Default::default()
            },
            services: vec![ServiceDefinition {
                name: "smtp".to_string(),
                image: "trydirect/smtp".to_string(),
                ports: vec!["1025:25".to_string()],
                environment: HashMap::from([
                    (
                        "RELAY_NETWORKS".to_string(),
                        ":127.0.0.0/8:10.0.0.0/8:172.16.0.0/12:192.168.0.0/16".to_string(),
                    ),
                    ("PORT".to_string(), "25".to_string()),
                ]),
                volumes: vec!["smtp_data:/data".to_string()],
                depends_on: Vec::new(),
                command: None,
                healthcheck: None,
            }],
            ..Default::default()
        };

        let result =
            sync_configured_compose_services(dir.path(), &config, &[String::from("smtp")]).unwrap();

        assert_eq!(result.updated_services, vec!["smtp"]);
        assert!(result.backup_path.unwrap().exists());
        let updated = std::fs::read_to_string(dir.path().join("docker-compose.yml")).unwrap();
        assert!(updated.contains("smtp:"));
        assert!(updated.contains("image: trydirect/smtp"));
        assert!(updated.contains("\"1025:25\"") || updated.contains("1025:25"));
        assert!(updated.contains("RELAY_NETWORKS"));
        assert!(updated.contains("default_network"));
        assert!(updated.contains("smtp_data:"));
    }
}
