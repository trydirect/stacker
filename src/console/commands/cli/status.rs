use std::path::Path;

use crate::cli::config_parser::{CloudOrchestrator, DeployTarget, StackerConfig};
use crate::cli::credentials::CredentialsManager;
use crate::cli::error::CliError;
use crate::cli::install_runner::{CommandExecutor, CommandOutput, ShellExecutor};
use crate::cli::stacker_client::{self, DeploymentStatusInfo, StackerClient};
use crate::console::commands::CallableTrait;

/// Output directory for generated artifacts.
const OUTPUT_DIR: &str = ".stacker";
const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// `stacker status [--json] [--watch]`
///
/// Shows the current deployment status.
///
/// - **Local deployments**: runs `docker compose ps` for container status.
/// - **Cloud deployments**: queries the Stacker server API for deployment
///   progress (pending → in_progress → completed / failed).
///   When `--watch` is used, polls every 5 seconds until a terminal status.
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

/// Core status logic for **local** deployments, extracted for testability.
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

// ── Cloud deployment status ─────────────────────────

/// Terminal statuses — once reached, `--watch` stops polling.
const TERMINAL_STATUSES: &[&str] = &[
    "completed",
    "failed",
    "cancelled",
    "error",
    "paused",
];

/// Check if a status is terminal (deployment finished or failed).
fn is_terminal(status: &str) -> bool {
    TERMINAL_STATUSES.iter().any(|s| *s == status)
}

/// Pretty-print a deployment status to stderr.
fn print_deployment_status(info: &DeploymentStatusInfo, json: bool) {
    if json {
        if let Ok(j) = serde_json::to_string_pretty(info) {
            println!("{}", j);
        }
    } else {
        let status_icon = match info.status.as_str() {
            "completed" => "✓",
            "failed" | "error" | "cancelled" => "✗",
            "in_progress" => "⟳",
            "pending" | "wait_start" => "◷",
            "paused" | "wait_resume" => "⏸",
            "confirmed" => "✓",
            _ => "?",
        };

        println!(
            "{} Deployment #{} — status: {}",
            status_icon, info.id, info.status
        );
        if let Some(ref msg) = info.status_message {
            println!("  Message:         {}", msg);
        }
        println!("  Project ID:      {}", info.project_id);
        println!("  Deployment hash: {}", info.deployment_hash);
        println!("  Created:         {}", info.created_at);
        println!("  Updated:         {}", info.updated_at);
    }
}

/// Resolve the project name from stacker.yml (same logic as deploy).
fn resolve_project_name(config: &StackerConfig) -> String {
    config
        .project
        .identity
        .clone()
        .unwrap_or_else(|| config.name.clone())
}

/// Query cloud deployment status from the Stacker server, optionally watching.
fn run_cloud_status(json: bool, watch: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Load stacker.yml to find project name
    let project_dir = std::env::current_dir()?;
    let config_path = project_dir.join(DEFAULT_CONFIG_FILE);

    if !config_path.exists() {
        return Err(Box::new(CliError::ConfigValidation(
            "No stacker.yml found. Run 'stacker init' first.".to_string(),
        )));
    }

    let config_str = std::fs::read_to_string(&config_path)?;
    let config: StackerConfig = serde_yaml::from_str(&config_str).map_err(|e| {
        CliError::ConfigValidation(format!("Invalid stacker.yml: {}", e))
    })?;

    let project_name = resolve_project_name(&config);

    // Load credentials
    let cred_manager = CredentialsManager::with_default_store();
    let creds = cred_manager.require_valid_token("deployment status")?;

    let base_url = stacker_client::DEFAULT_STACKER_URL.to_string();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            CliError::DeployFailed {
                target: DeployTarget::Cloud,
                reason: format!("Failed to initialize async runtime: {}", e),
            }
        })?;

    rt.block_on(async {
        let client = StackerClient::new(&base_url, &creds.access_token);

        // Resolve project ID by name
        let project = client.find_project_by_name(&project_name).await?;
        let project = project.ok_or_else(|| CliError::DeployFailed {
            target: DeployTarget::Cloud,
            reason: format!(
                "Project '{}' not found on server. Deploy first with 'stacker deploy --target cloud'.",
                project_name
            ),
        })?;

        if !watch {
            // Single query
            let status = client
                .get_deployment_status_by_project(project.id)
                .await?;
            match status {
                Some(info) => {
                    print_deployment_status(&info, json);
                    Ok(())
                }
                None => {
                    eprintln!("No deployments found for project '{}' (id={})", project_name, project.id);
                    Ok(())
                }
            }
        } else {
            // Watch mode — poll every 5 seconds
            eprintln!(
                "Watching deployment status for project '{}' (id={})...\n",
                project_name, project.id
            );

            let poll_interval = std::time::Duration::from_secs(5);
            let mut last_status = String::new();

            loop {
                let status = client
                    .get_deployment_status_by_project(project.id)
                    .await?;

                match status {
                    Some(info) => {
                        if info.status != last_status {
                            print_deployment_status(&info, json);
                            last_status = info.status.clone();
                        }

                        if is_terminal(&info.status) {
                            if !json {
                                eprintln!("\nDeployment reached terminal status: {}", info.status);
                            }
                            return Ok(());
                        }
                    }
                    None => {
                        if last_status.is_empty() {
                            eprintln!("No deployments found yet. Waiting...");
                            last_status = "<none>".to_string();
                        }
                    }
                }

                tokio::time::sleep(poll_interval).await;
            }
        }
    })
}

/// Detect whether the project is configured for cloud (remote) deployment.
fn is_cloud_deployment(project_dir: &Path) -> bool {
    let config_path = project_dir.join(DEFAULT_CONFIG_FILE);
    if !config_path.exists() {
        return false;
    }

    let config_str = match std::fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let config: StackerConfig = match serde_yaml::from_str(&config_str) {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Cloud if target is Cloud, or if remote orchestrator is configured
    if config.deploy.target == DeployTarget::Cloud {
        return true;
    }

    if let Some(cloud_cfg) = &config.deploy.cloud {
        if cloud_cfg.orchestrator == CloudOrchestrator::Remote {
            return true;
        }
    }

    false
}

impl CallableTrait for StatusCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = std::env::current_dir()?;

        if is_cloud_deployment(&project_dir) {
            // Cloud deployment — query Stacker server
            run_cloud_status(self.json, self.watch)?;
        } else {
            // Local deployment — docker compose ps
            let executor = ShellExecutor;
            let output = run_status(&project_dir, self.json, &executor)?;
            print!("{}", output.stdout);

            if self.watch {
                eprintln!("Note: --watch is only supported for cloud deployments.");
            }
        }

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

    #[test]
    fn test_is_terminal_status() {
        assert!(is_terminal("completed"));
        assert!(is_terminal("failed"));
        assert!(is_terminal("cancelled"));
        assert!(is_terminal("error"));
        assert!(is_terminal("paused"));
        assert!(!is_terminal("pending"));
        assert!(!is_terminal("in_progress"));
        assert!(!is_terminal("wait_start"));
    }

    #[test]
    fn test_is_cloud_deployment_no_config() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(!is_cloud_deployment(dir.path()));
    }
}
