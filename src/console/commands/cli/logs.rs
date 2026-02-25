use std::path::Path;

use crate::cli::error::CliError;
use crate::cli::install_runner::{CommandExecutor, CommandOutput, ShellExecutor};
use crate::console::commands::CallableTrait;

/// Output directory for generated artifacts.
const OUTPUT_DIR: &str = ".stacker";

/// `stacker logs [--service <name>] [--follow] [--tail <n>] [--since <duration>]`
///
/// Shows container logs for the deployed stack (delegates to docker compose logs).
pub struct LogsCommand {
    pub service: Option<String>,
    pub follow: bool,
    pub tail: Option<u32>,
    pub since: Option<String>,
}

impl LogsCommand {
    pub fn new(
        service: Option<String>,
        follow: bool,
        tail: Option<u32>,
        since: Option<String>,
    ) -> Self {
        Self {
            service,
            follow,
            tail,
            since,
        }
    }
}

/// Build the `docker compose logs` argument list.
pub fn build_logs_args(
    compose_path: &str,
    service: Option<&str>,
    follow: bool,
    tail: Option<u32>,
    since: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "compose".to_string(),
        "-f".to_string(),
        compose_path.to_string(),
        "logs".to_string(),
    ];

    if follow {
        args.push("-f".to_string());
    }

    if let Some(n) = tail {
        args.push("--tail".to_string());
        args.push(n.to_string());
    }

    if let Some(s) = since {
        args.push("--since".to_string());
        args.push(s.to_string());
    }

    if let Some(svc) = service {
        args.push(svc.to_string());
    }

    args
}

/// Core logic, extracted for testability.
pub fn run_logs(
    project_dir: &Path,
    service: Option<&str>,
    follow: bool,
    tail: Option<u32>,
    since: Option<&str>,
    executor: &dyn CommandExecutor,
) -> Result<CommandOutput, CliError> {
    let compose_path = project_dir.join(OUTPUT_DIR).join("docker-compose.yml");

    if !compose_path.exists() {
        return Err(CliError::ConfigValidation(
            "No deployment found. Run 'stacker deploy' first.".to_string(),
        ));
    }

    let compose_str = compose_path.to_string_lossy().to_string();
    let args = build_logs_args(&compose_str, service, follow, tail, since);
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let output = executor.execute("docker", &args_refs)?;
    Ok(output)
}

impl CallableTrait for LogsCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = std::env::current_dir()?;
        let executor = ShellExecutor;

        let output = run_logs(
            &project_dir,
            self.service.as_deref(),
            self.follow,
            self.tail,
            self.since.as_deref(),
            &executor,
        )?;

        print!("{}", output.stdout);
        if !output.stderr.is_empty() {
            eprint!("{}", output.stderr);
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logs_constructs_compose_command() {
        let args = build_logs_args("/path/compose.yml", None, false, None, None);
        assert_eq!(args, vec!["compose", "-f", "/path/compose.yml", "logs"]);
    }

    #[test]
    fn test_logs_with_service_filter() {
        let args = build_logs_args("/path/compose.yml", Some("postgres"), false, None, None);
        assert!(args.contains(&"postgres".to_string()));
    }

    #[test]
    fn test_logs_with_follow() {
        let args = build_logs_args("/path/compose.yml", None, true, None, None);
        assert!(args.contains(&"-f".to_string()));
    }

    #[test]
    fn test_logs_with_tail() {
        let args = build_logs_args("/path/compose.yml", None, false, Some(100), None);
        assert!(args.contains(&"--tail".to_string()));
        assert!(args.contains(&"100".to_string()));
    }

    #[test]
    fn test_logs_with_since() {
        let args = build_logs_args("/path/compose.yml", None, false, None, Some("1h"));
        assert!(args.contains(&"--since".to_string()));
        assert!(args.contains(&"1h".to_string()));
    }

    #[test]
    fn test_logs_no_deployment_returns_error() {
        use crate::cli::install_runner::CommandOutput;
        use std::sync::Mutex;

        struct MockExec;
        impl CommandExecutor for MockExec {
            fn execute(&self, _p: &str, _a: &[&str]) -> Result<CommandOutput, CliError> {
                Ok(CommandOutput { exit_code: 0, stdout: String::new(), stderr: String::new() })
            }
        }

        let dir = tempfile::TempDir::new().unwrap();
        let result = run_logs(dir.path(), None, false, None, None, &MockExec);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("No deployment found") || err.contains("deploy"));
    }
}
