use std::path::{Path, PathBuf};
use std::io::{self, Write};

use crate::cli::config_parser::{
    CloudConfig, CloudProvider, DeployTarget, ServerConfig, StackerConfig,
};
use crate::cli::error::CliError;
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
                    .unwrap_or_else(|| "cx22".to_string());
                let size = prompt_with_default("Cloud size", &size_default)?;

                let ssh_key = config
                    .deploy
                    .cloud
                    .as_ref()
                    .and_then(|c| c.ssh_key.clone());

                config.deploy.target = DeployTarget::Cloud;
                config.deploy.cloud = Some(CloudConfig {
                    provider,
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
                    ssh_key,
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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn minimal_config_yaml() -> &'static str {
        "name: test-app\nversion: \"1.0\"\napp:\n  type: static\n  source: \"./dist\"\ndeploy:\n  target: local\n"
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
}
