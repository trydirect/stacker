use std::path::{Path, PathBuf};
use std::io::{self, Write};

use crate::cli::config_parser::{
    CloudConfig, CloudOrchestrator, CloudProvider, DeployTarget, ServerConfig, StackerConfig,
};
use crate::cli::error::CliError;
use crate::console::commands::cli::init::full_config_reference_example;
use crate::console::commands::CallableTrait;

const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// Resolve config path from optional override.
fn resolve_config_path(file: &Option<String>) -> String {
    file.as_deref()
        .unwrap_or(DEFAULT_CONFIG_FILE)
        .to_string()
}

fn prompt_line(prompt: &str) -> Result<String, CliError> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_with_default(prompt: &str, default: &str) -> Result<String, CliError> {
    let line = prompt_line(&format!("{} [{}]: ", prompt, default))?;
    if line.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(line)
    }
}

fn parse_cloud_provider(s: &str) -> Result<CloudProvider, CliError> {
    let json = format!("\"{}\"", s.trim().to_lowercase());
    serde_json::from_str::<CloudProvider>(&json).map_err(|_| {
        CliError::ConfigValidation(
            "Invalid cloud provider. Use: hetzner, digitalocean, aws, linode, vultr"
                .to_string(),
        )
    })
}

fn default_region_for_provider(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::Hetzner => "nbg1",
        CloudProvider::Digitalocean => "fra1",
        CloudProvider::Aws => "us-east-1",
        CloudProvider::Linode => "us-east",
        CloudProvider::Vultr => "ewr",
    }
}

fn default_size_for_provider(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::Hetzner => "cx11",
        CloudProvider::Digitalocean => "s-1vcpu-2gb",
        CloudProvider::Aws => "t3.small",
        CloudProvider::Linode => "g6-standard-2",
        CloudProvider::Vultr => "vc2-2c-4gb",
    }
}

fn sanitize_stack_code(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;

    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }

    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "app-stack".to_string()
    } else {
        out
    }
}

fn provider_code_for_remote(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::Hetzner => "htz",
        CloudProvider::Digitalocean => "do",
        CloudProvider::Aws => "aws",
        CloudProvider::Linode => "lo",
        CloudProvider::Vultr => "vu",
    }
}

fn first_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn resolve_remote_cloud_credentials(provider_code: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut creds = serde_json::Map::new();

    match provider_code {
        "htz" => {
            if let Some(token) = first_non_empty_env(&[
                "STACKER_CLOUD_TOKEN",
                "STACKER_HETZNER_TOKEN",
                "HETZNER_TOKEN",
                "HCLOUD_TOKEN",
            ]) {
                creds.insert("cloud_token".to_string(), serde_json::Value::String(token));
            }
        }
        "do" => {
            if let Some(token) = first_non_empty_env(&[
                "STACKER_CLOUD_TOKEN",
                "STACKER_DIGITALOCEAN_TOKEN",
                "DIGITALOCEAN_TOKEN",
                "DO_API_TOKEN",
            ]) {
                creds.insert("cloud_token".to_string(), serde_json::Value::String(token));
            }
        }
        "lo" => {
            if let Some(token) = first_non_empty_env(&[
                "STACKER_CLOUD_TOKEN",
                "STACKER_LINODE_TOKEN",
                "LINODE_TOKEN",
            ]) {
                creds.insert("cloud_token".to_string(), serde_json::Value::String(token));
            }
        }
        "vu" => {
            if let Some(token) = first_non_empty_env(&[
                "STACKER_CLOUD_TOKEN",
                "STACKER_VULTR_TOKEN",
                "VULTR_TOKEN",
                "VULTR_API_KEY",
            ]) {
                creds.insert("cloud_token".to_string(), serde_json::Value::String(token));
            }
        }
        "aws" => {
            if let Some(key) = first_non_empty_env(&["STACKER_CLOUD_KEY", "AWS_ACCESS_KEY_ID"]) {
                creds.insert("cloud_key".to_string(), serde_json::Value::String(key));
            }
            if let Some(secret) =
                first_non_empty_env(&["STACKER_CLOUD_SECRET", "AWS_SECRET_ACCESS_KEY"])
            {
                creds.insert("cloud_secret".to_string(), serde_json::Value::String(secret));
            }
        }
        _ => {}
    }

    creds
}

pub fn run_generate_remote_payload(
    config_path: &str,
    output: Option<&str>,
) -> Result<Vec<String>, CliError> {
    let path = Path::new(config_path);
    if !path.exists() {
        return Err(CliError::ConfigNotFound {
            path: PathBuf::from(config_path),
        });
    }

    let mut config = StackerConfig::from_file(path)?;
    let config_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let output_path = match output {
        Some(out) => {
            let p = PathBuf::from(out);
            if p.is_absolute() {
                p
            } else {
                config_dir.join(p)
            }
        }
        None => config_dir.join("stacker.remote.deploy.json"),
    };

    let cloud = config.deploy.cloud.clone();
    let provider = cloud
        .as_ref()
        .map(|c| c.provider)
        .unwrap_or(CloudProvider::Hetzner);
    let region = cloud
        .as_ref()
        .and_then(|c| c.region.clone())
        .unwrap_or_else(|| default_region_for_provider(provider).to_string());
    let size = cloud
        .as_ref()
        .and_then(|c| c.size.clone())
        .unwrap_or_else(|| default_size_for_provider(provider).to_string());
    let stack_code = config
        .project
        .identity
        .clone()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "custom-stack".to_string());
    let provider_code = provider_code_for_remote(provider);
    let os = match provider_code {
        "do" => "docker-20-04",
        _ => "ubuntu-22.04",
    };

    let mut payload = serde_json::json!({
        "provider": provider_code,
        "region": region,
        "server": size,
        "os": os,
        "ssl": "letsencrypt",
        "commonDomain": format!("{}.example.com", sanitize_stack_code(&config.name)),
        "domainList": {},
        "stack_code": stack_code,
        "project_name": config.name,
        "selected_plan": "free",
        "payment_type": "subscription",
        "subscriptions": [],
        "vars": [],
        "integrated_features": [],
        "extended_features": [],
        "save_token": true,
        "custom": {
            "project_name": config.name,
            "custom_stack_code": sanitize_stack_code(&config.name),
            "project_overview": format!("Generated by stacker-cli for {}", config.name)
        }
    });

    if let Some(obj) = payload.as_object_mut() {
        for (key, value) in resolve_remote_cloud_credentials(provider_code) {
            obj.insert(key, value);
        }
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload_str = serde_json::to_string_pretty(&payload)
        .map_err(|e| CliError::ConfigValidation(format!("Failed to serialize payload: {}", e)))?;
    std::fs::write(&output_path, payload_str)?;

    let remote_payload_file = output_path
        .strip_prefix(config_dir)
        .map(PathBuf::from)
        .unwrap_or_else(|_| output_path.clone());

    let existing_cloud = config.deploy.cloud.clone().unwrap_or(CloudConfig {
        provider,
        orchestrator: CloudOrchestrator::Remote,
        region: Some(default_region_for_provider(provider).to_string()),
        size: Some(default_size_for_provider(provider).to_string()),
        install_image: None,
        remote_payload_file: None,
        ssh_key: None,
        key: None,
        server: None,
    });

    config.deploy.target = DeployTarget::Cloud;
    config.deploy.cloud = Some(CloudConfig {
        provider: existing_cloud.provider,
        orchestrator: CloudOrchestrator::Remote,
        region: existing_cloud.region,
        size: existing_cloud.size,
        install_image: existing_cloud.install_image,
        remote_payload_file: Some(remote_payload_file),
        ssh_key: existing_cloud.ssh_key,
        key: existing_cloud.key,
        server: existing_cloud.server,
    });

    let backup_path = format!("{}.bak", config_path);
    std::fs::copy(config_path, &backup_path)?;
    let yaml = serde_yaml::to_string(&config)
        .map_err(|e| CliError::ConfigValidation(format!("Failed to serialize config: {}", e)))?;
    std::fs::write(config_path, yaml)?;

    Ok(vec![
        format!(
            "Generated remote payload (advanced/debug): {}",
            output_path.display()
        ),
        "Set deploy.target=cloud and deploy.cloud.orchestrator=remote (advanced mode)"
            .to_string(),
        "Tip: regular users can skip this and run `stacker deploy --target cloud` directly"
            .to_string(),
        format!("Backup written to {}", backup_path),
    ])
}

fn apply_cloud_settings(
    config: &mut StackerConfig,
    provider: CloudProvider,
    region: Option<String>,
    size: Option<String>,
    ssh_key: Option<PathBuf>,
) {
    let existing_orchestrator = config
        .deploy
        .cloud
        .as_ref()
        .map(|c| c.orchestrator)
        .unwrap_or(CloudOrchestrator::Local);
    let existing_install_image = config
        .deploy
        .cloud
        .as_ref()
        .and_then(|c| c.install_image.clone());

    let existing_remote_payload_file = config
        .deploy
        .cloud
        .as_ref()
        .and_then(|c| c.remote_payload_file.clone());

    config.deploy.target = DeployTarget::Cloud;
    config.deploy.cloud = Some(CloudConfig {
        provider,
        orchestrator: existing_orchestrator,
        region,
        size,
        install_image: existing_install_image,
        remote_payload_file: existing_remote_payload_file,
        ssh_key,
        key: None,
        server: None,
    });
}

pub fn run_setup_cloud_interactive(config_path: &str) -> Result<Vec<String>, CliError> {
    let path = Path::new(config_path);
    if !path.exists() {
        return Err(CliError::ConfigNotFound {
            path: PathBuf::from(config_path),
        });
    }

    let mut config = StackerConfig::from_file(path)?;
    let mut applied = Vec::new();

    eprintln!("Cloud setup wizard:");

    let provider_default = config
        .deploy
        .cloud
        .as_ref()
        .map(|c| c.provider)
        .unwrap_or(CloudProvider::Hetzner);

    let provider_input = prompt_with_default(
        "Cloud provider (hetzner|digitalocean|aws|linode|vultr)",
        &provider_default.to_string(),
    )?;
    let provider = parse_cloud_provider(&provider_input)?;

    let region_default = config
        .deploy
        .cloud
        .as_ref()
        .and_then(|c| c.region.clone())
        .unwrap_or_else(|| default_region_for_provider(provider).to_string());
    let region = prompt_with_default("Cloud region", &region_default)?;

    let size_default = config
        .deploy
        .cloud
        .as_ref()
        .and_then(|c| c.size.clone())
        .unwrap_or_else(|| default_size_for_provider(provider).to_string());
    let size = prompt_with_default("Cloud size", &size_default)?;

    let ssh_key_default = config
        .deploy
        .cloud
        .as_ref()
        .and_then(|c| c.ssh_key.clone())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.ssh/id_rsa".to_string());
    let ssh_key_input = prompt_with_default(
        "SSH key path (leave empty to skip)",
        &ssh_key_default,
    )?;

    let region_opt = if region.trim().is_empty() {
        None
    } else {
        Some(region)
    };
    let size_opt = if size.trim().is_empty() {
        None
    } else {
        Some(size)
    };
    let ssh_key_opt = if ssh_key_input.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(ssh_key_input))
    };

    apply_cloud_settings(&mut config, provider, region_opt, size_opt, ssh_key_opt);

    let backup_path = format!("{}.bak", config_path);
    std::fs::copy(config_path, &backup_path)?;

    let yaml = serde_yaml::to_string(&config)
        .map_err(|e| CliError::ConfigValidation(format!("Failed to serialize config: {}", e)))?;
    std::fs::write(config_path, yaml)?;

    applied.push("Set deploy.target=cloud and deploy.cloud.*".to_string());
    applied.push(format!("Backup written to {}", backup_path));
    Ok(applied)
}

/// Interactive fixer for common missing required fields.
///
/// Current MVP handles:
/// - E001: missing deploy.cloud.provider
/// - E002: missing deploy.server.host
pub fn run_fix_interactive(config_path: &str) -> Result<Vec<String>, CliError> {
    let path = Path::new(config_path);
    if !path.exists() {
        return Err(CliError::ConfigNotFound {
            path: PathBuf::from(config_path),
        });
    }

    let mut config = StackerConfig::from_file(path)?;
    let issues = config.validate_semantics();
    let mut applied = Vec::new();

    if issues.is_empty() {
        return Ok(applied);
    }

    for issue in &issues {
        match issue.code.as_str() {
            "E001" => {
                eprintln!("Detected missing cloud provider settings (E001).");

                let provider_default = config
                    .deploy
                    .cloud
                    .as_ref()
                    .map(|c| c.provider.to_string())
                    .unwrap_or_else(|| "hetzner".to_string());

                let provider_input = prompt_with_default(
                    "Cloud provider (hetzner|digitalocean|aws|linode|vultr)",
                    &provider_default,
                )?;
                let provider = parse_cloud_provider(&provider_input)?;

                let region_default = config
                    .deploy
                    .cloud
                    .as_ref()
                    .and_then(|c| c.region.clone())
                    .unwrap_or_else(|| "nbg1".to_string());
                let region = prompt_with_default("Cloud region", &region_default)?;

                let size_default = config
                    .deploy
                    .cloud
                    .as_ref()
                    .and_then(|c| c.size.clone())
                    .unwrap_or_else(|| "cx11".to_string());
                let size = prompt_with_default("Cloud size", &size_default)?;

                let ssh_key = config
                    .deploy
                    .cloud
                    .as_ref()
                    .and_then(|c| c.ssh_key.clone());

                let orchestrator = config
                    .deploy
                    .cloud
                    .as_ref()
                    .map(|c| c.orchestrator)
                    .unwrap_or(CloudOrchestrator::Local);

                let install_image = config
                    .deploy
                    .cloud
                    .as_ref()
                    .and_then(|c| c.install_image.clone());

                let remote_payload_file = config
                    .deploy
                    .cloud
                    .as_ref()
                    .and_then(|c| c.remote_payload_file.clone());

                config.deploy.target = DeployTarget::Cloud;
                config.deploy.cloud = Some(CloudConfig {
                    provider,
                    orchestrator,
                    region: if region.trim().is_empty() {
                        None
                    } else {
                        Some(region)
                    },
                    size: if size.trim().is_empty() {
                        None
                    } else {
                        Some(size)
                    },
                    install_image,
                    remote_payload_file,
                    ssh_key,
                    key: None,
                    server: None,
                });

                applied.push("Set deploy.target=cloud and deploy.cloud.*".to_string());
            }
            "E002" => {
                eprintln!("Detected missing server host settings (E002).");

                let mut host = config
                    .deploy
                    .server
                    .as_ref()
                    .map(|s| s.host.clone())
                    .unwrap_or_default();

                while host.trim().is_empty() {
                    host = prompt_line("Server host (required, e.g. 203.0.113.10): ")?;
                }

                let user_default = config
                    .deploy
                    .server
                    .as_ref()
                    .map(|s| s.user.clone())
                    .unwrap_or_else(|| "root".to_string());
                let user = prompt_with_default("SSH user", &user_default)?;

                let port_default = config
                    .deploy
                    .server
                    .as_ref()
                    .map(|s| s.port.to_string())
                    .unwrap_or_else(|| "22".to_string());
                let port_input = prompt_with_default("SSH port", &port_default)?;
                let port = port_input.parse::<u16>().unwrap_or(22);

                let ssh_key = config
                    .deploy
                    .server
                    .as_ref()
                    .and_then(|s| s.ssh_key.clone());

                config.deploy.target = DeployTarget::Server;
                config.deploy.server = Some(ServerConfig {
                    host,
                    user,
                    ssh_key,
                    port,
                });

                applied.push("Set deploy.target=server and deploy.server.*".to_string());
            }
            _ => {}
        }
    }

    if applied.is_empty() {
        return Ok(applied);
    }

    let backup_path = format!("{}.bak", config_path);
    std::fs::copy(config_path, &backup_path)?;

    let yaml = serde_yaml::to_string(&config)
        .map_err(|e| CliError::ConfigValidation(format!("Failed to serialize config: {}", e)))?;
    std::fs::write(config_path, yaml)?;

    applied.push(format!("Backup written to {}", backup_path));
    Ok(applied)
}

/// Core validate logic — loads config, runs semantic checks, returns issues.
pub fn run_validate(config_path: &str) -> Result<Vec<String>, CliError> {
    let path = Path::new(config_path);
    if !path.exists() {
        return Err(CliError::ConfigNotFound {
            path: PathBuf::from(config_path),
        });
    }

    let config = StackerConfig::from_file(path)?;
    let issues = config.validate_semantics();
    let messages: Vec<String> = issues.iter().map(|i| format!("{:?}", i)).collect();
    Ok(messages)
}

/// Core show logic — loads config, serialises to YAML string.
pub fn run_show(config_path: &str) -> Result<String, CliError> {
    let path = Path::new(config_path);
    if !path.exists() {
        return Err(CliError::ConfigNotFound {
            path: PathBuf::from(config_path),
        });
    }

    let config = StackerConfig::from_file(path)?;
    let yaml = serde_yaml::to_string(&config).map_err(|e| {
        CliError::ConfigValidation(format!("Failed to serialize config: {}", e))
    })?;
    Ok(yaml)
}

/// `stacker config validate [--file stacker.yml]`
///
/// Validates a stacker.yml configuration file.
pub struct ConfigValidateCommand {
    pub file: Option<String>,
}

impl ConfigValidateCommand {
    pub fn new(file: Option<String>) -> Self {
        Self { file }
    }
}

impl CallableTrait for ConfigValidateCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = resolve_config_path(&self.file);
        let issues = run_validate(&path)?;

        if issues.is_empty() {
            eprintln!("✓ Configuration is valid");
        } else {
            eprintln!("Configuration issues:");
            for issue in &issues {
                eprintln!("  - {}", issue);
            }
        }

        Ok(())
    }
}

/// `stacker config show [--file stacker.yml]`
///
/// Displays the resolved configuration (with env vars substituted).
pub struct ConfigShowCommand {
    pub file: Option<String>,
}

/// `stacker config fix [--file stacker.yml] [--interactive]`
///
/// Interactively repairs common missing required fields in stacker.yml.
pub struct ConfigFixCommand {
    pub file: Option<String>,
    pub interactive: bool,
}

/// `stacker config setup cloud [--file stacker.yml]`
///
/// Interactive cloud setup wizard that writes deploy.target/deploy.cloud.
pub struct ConfigSetupCloudCommand {
    pub file: Option<String>,
}

impl ConfigSetupCloudCommand {
    pub fn new(file: Option<String>) -> Self {
        Self { file }
    }
}

impl CallableTrait for ConfigSetupCloudCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = resolve_config_path(&self.file);
        let applied = run_setup_cloud_interactive(&path)?;

        eprintln!("✓ Updated {}", path);
        for item in applied {
            eprintln!("  - {}", item);
        }
        eprintln!("Run: stacker config validate");
        Ok(())
    }
}

/// `stacker config setup remote-payload [--file stacker.yml] [--out stacker.remote.deploy.json]`
///
/// Advanced/debug helper: generate a User Service `/install/init/` payload file and wire config for remote orchestrator.
pub struct ConfigSetupRemotePayloadCommand {
    pub file: Option<String>,
    pub out: Option<String>,
}

impl ConfigSetupRemotePayloadCommand {
    pub fn new(file: Option<String>, out: Option<String>) -> Self {
        Self { file, out }
    }
}

impl CallableTrait for ConfigSetupRemotePayloadCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = resolve_config_path(&self.file);
        let applied = run_generate_remote_payload(&path, self.out.as_deref())?;

        eprintln!("✓ Updated {}", path);
        for item in applied {
            eprintln!("  - {}", item);
        }
        eprintln!("Run: stacker deploy --target cloud");
        eprintln!("Note: this command is mainly for troubleshooting and integrations.");
        Ok(())
    }
}

impl ConfigFixCommand {
    pub fn new(file: Option<String>, interactive: bool) -> Self {
        Self { file, interactive }
    }
}

impl CallableTrait for ConfigFixCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.interactive {
            return Err(Box::new(CliError::ConfigValidation(
                "Only interactive mode is supported for now. Use: stacker config fix --interactive"
                    .to_string(),
            )));
        }

        let path = resolve_config_path(&self.file);
        let applied = run_fix_interactive(&path)?;

        if applied.is_empty() {
            eprintln!("No interactive fixes were applied.");
        } else {
            eprintln!("✓ Updated {}", path);
            for item in applied {
                eprintln!("  - {}", item);
            }
            eprintln!("Run: stacker config validate");
        }

        Ok(())
    }
}

impl ConfigShowCommand {
    pub fn new(file: Option<String>) -> Self {
        Self { file }
    }
}

impl CallableTrait for ConfigShowCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = resolve_config_path(&self.file);
        let yaml = run_show(&path)?;
        println!("{}", yaml);
        Ok(())
    }
}

/// `stacker config example`
///
/// Prints a full commented `stacker.yml` reference example.
pub struct ConfigExampleCommand;

impl ConfigExampleCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CallableTrait for ConfigExampleCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", full_config_reference_example());
        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn minimal_config_yaml() -> &'static str {
        "name: test-app\nversion: \"1.0\"\nproject:\n  identity: \"registered-stack-code\"\napp:\n  type: static\n  source: \"./dist\"\ndeploy:\n  target: local\n"
    }

    fn write_config(dir: &Path, content: &str) -> String {
        let path = dir.join("stacker.yml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn test_validate_returns_ok_for_valid_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = write_config(dir.path(), minimal_config_yaml());
        let result = run_validate(&path).unwrap();
        // Minimal valid config should have zero or few issues
        assert!(result.len() < 5);
    }

    #[test]
    fn test_validate_missing_file_returns_error() {
        let result = run_validate("/nonexistent/stacker.yml");
        assert!(result.is_err());
    }

    #[test]
    fn test_show_returns_yaml_string() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = write_config(dir.path(), minimal_config_yaml());
        let yaml = run_show(&path).unwrap();
        assert!(yaml.contains("test-app"));
    }

    #[test]
    fn test_show_missing_file_returns_error() {
        let result = run_show("/nonexistent/stacker.yml");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_config_path_default() {
        let resolved = resolve_config_path(&None);
        assert_eq!(resolved, "stacker.yml");
    }

    #[test]
    fn test_resolve_config_path_override() {
        let resolved = resolve_config_path(&Some("custom.yml".to_string()));
        assert_eq!(resolved, "custom.yml");
    }

    #[test]
    fn test_parse_cloud_provider_valid() {
        assert_eq!(parse_cloud_provider("hetzner").unwrap(), CloudProvider::Hetzner);
        assert_eq!(parse_cloud_provider("AWS").unwrap(), CloudProvider::Aws);
    }

    #[test]
    fn test_parse_cloud_provider_invalid() {
        let result = parse_cloud_provider("gcp");
        assert!(result.is_err());
    }

    #[test]
    fn test_default_region_for_provider() {
        assert_eq!(default_region_for_provider(CloudProvider::Hetzner), "nbg1");
        assert_eq!(default_region_for_provider(CloudProvider::Aws), "us-east-1");
    }

    #[test]
    fn test_apply_cloud_settings_sets_target_and_cloud() {
        let mut cfg = StackerConfig::from_str(minimal_config_yaml()).unwrap();
        apply_cloud_settings(
            &mut cfg,
            CloudProvider::Hetzner,
            Some("nbg1".to_string()),
            Some("cx11".to_string()),
            None,
        );

        assert_eq!(cfg.deploy.target, DeployTarget::Cloud);
        let cloud = cfg.deploy.cloud.unwrap();
        assert_eq!(cloud.provider, CloudProvider::Hetzner);
        assert_eq!(cloud.region.as_deref(), Some("nbg1"));
        assert_eq!(cloud.size.as_deref(), Some("cx11"));
    }

    #[test]
    fn test_run_generate_remote_payload_writes_file_and_updates_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = write_config(dir.path(), minimal_config_yaml());

        let applied = run_generate_remote_payload(&config_path, Some("stacker.remote.deploy.json")).unwrap();
        assert!(!applied.is_empty());

        let payload_path = dir.path().join("stacker.remote.deploy.json");
        assert!(payload_path.exists());

        let payload_raw = std::fs::read_to_string(&payload_path).unwrap();
        let payload_json: serde_json::Value = serde_json::from_str(&payload_raw).unwrap();
        assert!(payload_json.get("provider").is_some());
        assert!(payload_json.get("commonDomain").is_some());
        assert!(payload_json.get("os").is_some());
        assert!(payload_json.get("selected_plan").is_some());
        assert!(payload_json.get("payment_type").is_some());
        assert!(payload_json.get("subscriptions").is_some());
        assert!(payload_json.get("stack_code").is_some());
        assert_eq!(
            payload_json.get("stack_code").and_then(|v| v.as_str()),
            Some("registered-stack-code")
        );

        let updated = StackerConfig::from_file(Path::new(&config_path)).unwrap();
        assert_eq!(updated.deploy.target, DeployTarget::Cloud);
        let cloud = updated.deploy.cloud.unwrap();
        assert_eq!(cloud.orchestrator, CloudOrchestrator::Remote);
        assert_eq!(
            cloud.remote_payload_file.as_deref(),
            Some(Path::new("stacker.remote.deploy.json"))
        );
    }
}
