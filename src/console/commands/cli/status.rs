use std::path::Path;

use crate::cli::config_parser::{DeployTarget, StackerConfig};
use crate::cli::error::CliError;
use crate::cli::install_runner::{CommandExecutor, CommandOutput, ShellExecutor};
use crate::console::commands::CallableTrait;

/// Output directory for generated artifacts.
const OUTPUT_DIR: &str = ".stacker";
const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// `stacker status [--json] [--watch]`
///
/// Shows the current deployment status (containers, health, ports).
pub struct StatusCommand {
    pub json: bool,
    pub watch: bool,
}

impl StatusCommand {
    pub fn new(json: bool, watch: bool) -> Self {
        Self { json, watch }
    }
}

/// Build `docker compose ps` arguments.
pub fn build_status_args(compose_path: &str, json: bool) -> Vec<String> {
    let mut args = vec![
        "compose".to_string(),
        "-f".to_string(),
        compose_path.to_string(),
        "ps".to_string(),
    ];

    if json {
        args.push("--format".to_string());
        args.push("json".to_string());
    }

    args
}

/// Core status logic, extracted for testability.
pub fn run_status(
    project_dir: &Path,
    json: bool,
    executor: &dyn CommandExecutor,
) -> Result<CommandOutput, CliError> {
    let compose_path = project_dir.join(OUTPUT_DIR).join("docker-compose.yml");

    if !compose_path.exists() {
        return Err(CliError::ConfigValidation(
            "No deployment found. Run 'stacker deploy' first.".to_string(),
        ));
    }

    let compose_str = compose_path.to_string_lossy().to_string();
    let args = build_status_args(&compose_str, json);
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let output = executor.execute("docker", &args_refs)?;
    Ok(output)
}

impl CallableTrait for StatusCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = std::env::current_dir()?;
        let executor = ShellExecutor;

        let output = run_status(&project_dir, self.json, &executor)?;
        print!("{}", output.stdout);

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_local_constructs_query() {
        let args = build_status_args("/path/compose.yml", false);
        assert_eq!(args, vec!["compose", "-f", "/path/compose.yml", "ps"]);
    }

    #[test]
    fn test_status_json_flag() {
        let args = build_status_args("/path/compose.yml", true);
        assert!(args.contains(&"--format".to_string()));
        assert!(args.contains(&"json".to_string()));
    }

    #[test]
    fn test_status_no_deployment_returns_error() {
        struct MockExec;
        impl CommandExecutor for MockExec {
            fn execute(&self, _p: &str, _a: &[&str]) -> Result<CommandOutput, CliError> {
                Ok(CommandOutput { exit_code: 0, stdout: String::new(), stderr: String::new() })
            }
        }

        let dir = tempfile::TempDir::new().unwrap();
        let result = run_status(dir.path(), false, &MockExec);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("No deployment found"));
    }
}
