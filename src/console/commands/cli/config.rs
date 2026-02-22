use std::path::{Path, PathBuf};

use crate::cli::config_parser::StackerConfig;
use crate::cli::error::CliError;
use crate::console::commands::CallableTrait;

const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// Resolve config path from optional override.
fn resolve_config_path(file: &Option<String>) -> String {
    file.as_deref()
        .unwrap_or(DEFAULT_CONFIG_FILE)
        .to_string()
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
}
