use std::path::Path;

use crate::cli::config_parser::DeployTarget;
use crate::cli::error::CliError;
use crate::cli::install_runner::{CommandExecutor, ShellExecutor};
use crate::cli::local_compose::{resolve_local_compose_path, resolve_local_compose_project_name};
use crate::console::commands::CallableTrait;

const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// `stacker destroy [--volumes] [--confirm]`
///
/// Tears down the deployed stack and optionally removes volumes.
pub struct DestroyCommand {
    pub volumes: bool,
    pub confirm: bool,
}

impl DestroyCommand {
    pub fn new(volumes: bool, confirm: bool) -> Self {
        Self { volumes, confirm }
    }
}

/// Build `docker compose down` arguments.
///
/// `project_name` MUST be passed via `-p` — without it Compose falls back to
/// the compose file's containing directory basename, which is the same
/// `.stacker/` for every project, defaulting to the shared project name
/// "stacker" for all of them. `down` on that shared scope removes/orphans
/// *any* running container whose service name happens to match one in the
/// current project's compose file, regardless of which project actually
/// started it — the same collision `LocalDeploy::deploy`/`destroy` in
/// install_runner.rs were fixed for. See GH issue #235.
pub fn build_destroy_args(compose_path: &str, project_name: &str, volumes: bool) -> Vec<String> {
    let mut args = vec![
        "compose".to_string(),
        "-p".to_string(),
        project_name.to_string(),
        "-f".to_string(),
        compose_path.to_string(),
        "down".to_string(),
    ];

    if volumes {
        args.push("--volumes".to_string());
    }

    args
}

/// Core destroy logic, extracted for testability.
pub fn run_destroy(
    project_dir: &Path,
    volumes: bool,
    confirm: bool,
    executor: &dyn CommandExecutor,
) -> Result<(), CliError> {
    if !confirm {
        return Err(CliError::ConfigValidation(
            "Destroy requires --confirm (-y) flag. This will remove all containers and data."
                .to_string(),
        ));
    }

    let compose_path = resolve_local_compose_path(project_dir).map_err(|err| match err {
        CliError::ConfigValidation(_) => {
            CliError::ConfigValidation("No deployment found. Nothing to destroy.".to_string())
        }
        other => other,
    })?;

    let compose_str = compose_path.to_string_lossy().to_string();
    let project_name = resolve_local_compose_project_name(project_dir);
    let args = build_destroy_args(&compose_str, &project_name, volumes);
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let output = executor.execute("docker", &args_refs)?;

    if !output.success() {
        return Err(CliError::DeployFailed {
            target: DeployTarget::Local,
            reason: format!("docker compose down failed: {}", output.stderr.trim()),
        });
    }

    Ok(())
}

impl CallableTrait for DestroyCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = std::env::current_dir()?;
        let executor = ShellExecutor;

        run_destroy(&project_dir, self.volumes, self.confirm, &executor)?;
        eprintln!("✓ Stack destroyed successfully");

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::install_runner::CommandOutput;
    use std::sync::Mutex;

    struct MockExecutor {
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }

        fn recorded_calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandExecutor for MockExecutor {
        fn execute(&self, program: &str, args: &[&str]) -> Result<CommandOutput, CliError> {
            self.calls.lock().unwrap().push((
                program.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
            ));
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn setup_with_compose() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        let stacker_dir = dir.path().join(".stacker");
        std::fs::create_dir_all(&stacker_dir).unwrap();
        std::fs::write(stacker_dir.join("docker-compose.yml"), "version: '3.8'\n").unwrap();
        dir
    }

    #[test]
    fn test_destroy_constructs_down_command() {
        let dir = setup_with_compose();
        let executor = MockExecutor::new();

        run_destroy(dir.path(), false, true, &executor).unwrap();

        let calls = executor.recorded_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "docker");
        assert!(calls[0].1.contains(&"down".to_string()));
    }

    #[test]
    fn test_destroy_with_volumes_flag() {
        let args = build_destroy_args("/path/compose.yml", "myproject", true);
        assert!(args.contains(&"--volumes".to_string()));
    }

    // Regression test for GH issue #235: `stacker destroy` previously never
    // passed `-p <project>`, so Compose fell back to the shared ".stacker"
    // directory-basename project name ("stacker") for every project — a
    // `destroy` in one project's directory could remove/orphan another,
    // unrelated project's containers sharing that same default scope.
    #[test]
    fn test_destroy_namespaces_compose_project_by_identity() {
        let dir = setup_with_compose();
        std::fs::write(
            dir.path().join(DEFAULT_CONFIG_FILE),
            "name: Miniflux Prod\ndeploy:\n  target: local\n",
        )
        .unwrap();
        let executor = MockExecutor::new();

        run_destroy(dir.path(), false, true, &executor).unwrap();

        let calls = executor.recorded_calls();
        assert_eq!(calls.len(), 1);
        let args = &calls[0].1;
        let p_index = args
            .iter()
            .position(|a| a == "-p")
            .expect("docker compose down should pass -p <project-name>");
        assert_eq!(
            args.get(p_index + 1).map(String::as_str),
            Some("miniflux-prod"),
            "project name should be derived from stacker.yml's name/identity, not the \
             compose file's directory, got args: {:?}",
            args
        );
    }

    #[test]
    fn test_destroy_uses_project_identity_over_name_for_project_name() {
        let dir = setup_with_compose();
        std::fs::write(
            dir.path().join(DEFAULT_CONFIG_FILE),
            "name: stacker\nproject:\n  identity: miniflux-blue\ndeploy:\n  target: local\n",
        )
        .unwrap();
        let executor = MockExecutor::new();

        run_destroy(dir.path(), false, true, &executor).unwrap();

        let calls = executor.recorded_calls();
        let args = &calls[0].1;
        let p_index = args.iter().position(|a| a == "-p").unwrap();
        assert_eq!(
            args.get(p_index + 1).map(String::as_str),
            Some("miniflux-blue")
        );
    }

    #[test]
    fn test_destroy_requires_confirmation() {
        let dir = setup_with_compose();
        let executor = MockExecutor::new();

        let result = run_destroy(dir.path(), false, false, &executor);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("confirm") || err.contains("Destroy"));
    }

    #[test]
    fn test_destroy_no_deployment_returns_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let executor = MockExecutor::new();

        let result = run_destroy(dir.path(), false, true, &executor);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("No deployment found") || err.contains("Nothing to destroy"));
    }

    #[test]
    fn test_destroy_uses_configured_compose_file_for_local_target() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("docker/local")).unwrap();
        std::fs::write(
            dir.path().join("docker/local/compose.yml"),
            "services: {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join(DEFAULT_CONFIG_FILE),
            "name: demo\ndeploy:\n  target: local\n  compose_file: docker/local/compose.yml\n",
        )
        .unwrap();

        let executor = MockExecutor::new();
        run_destroy(dir.path(), false, true, &executor).unwrap();

        let calls = executor.recorded_calls();
        assert_eq!(calls.len(), 1);
        let args = &calls[0].1;
        let f_index = args.iter().position(|a| a == "-f").unwrap();
        assert_eq!(
            args[f_index + 1],
            dir.path()
                .join("docker/local/compose.yml")
                .to_string_lossy()
        );
    }
}
