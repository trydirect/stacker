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
        // "Nothing to destroy" is true only when nothing was deployed. A
        // project whose last deploy went to a server or the cloud has a stack
        // running right now — it is simply out of this command's reach, since
        // destroy runs docker on *this* machine. Reporting it as absent sends
        // operators looking for broken local state while the remote stack keeps
        // running, and the server quietly accumulates orphaned containers.
        CliError::ConfigValidation(_) => {
            remote_teardown_guidance(project_dir).unwrap_or_else(|| {
                CliError::ConfigValidation("No deployment found. Nothing to destroy.".to_string())
            })
        }
        other => other,
    })?;

    let compose_str = compose_path.to_string_lossy().to_string();
    // The config names the project; Docker is the fallback when the config is
    // gone or unreadable. Guessing a name is not an option here — a wrong `-p`
    // matches nothing, `docker compose down` exits 0 on the empty project, and
    // destroy would report success over a stack that is still running.
    let project_name = match resolve_local_compose_project_name(project_dir) {
        Ok(name) => name,
        Err(config_err) => {
            crate::cli::local_compose::project_name_from_running_compose(&compose_path, executor)
                .ok_or_else(|| {
                    CliError::ConfigValidation(format!(
                        "{config_err}\n\nDocker also reports no running project started from {}. \
                 If the stack is still up, tear it down with the project name shown by \
                 `docker compose ls`.",
                        compose_path.display()
                    ))
                })?
        }
    };
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

/// Explain a remote deployment this command cannot reach, with the command
/// that does reach it.
///
/// Returns `None` when the project has no remote deployment recorded — then
/// "nothing to destroy" is the accurate answer after all.
///
/// `stacker destroy` is local-only: there is no CLI path that tears down a
/// server/cloud stack today (`stacker deployment` offers state/events/rollback,
/// and `DELETE /server/{id}` clears platform records — SSH key, agents,
/// billing — without touching the containers on the host). Until one exists,
/// the honest thing is to hand over the exact command.
fn remote_teardown_guidance(project_dir: &Path) -> Option<CliError> {
    let lock = crate::cli::deployment_lock::DeploymentLock::load_active(project_dir)
        .ok()
        .flatten()?;
    if lock.target == "local" {
        return None;
    }

    let host = lock.server_ip.as_deref().unwrap_or("<server-ip>");
    let user = lock.ssh_user.as_deref().unwrap_or("root");
    let key_hint = lock
        .ssh_key
        .as_ref()
        .map(|key| format!(" -i {}", key.display()))
        .unwrap_or_default();
    let port_hint = match lock.ssh_port {
        Some(port) if port != 22 => format!(" -p {port}"),
        _ => String::new(),
    };

    Some(CliError::ConfigValidation(format!(
        "This project's last deploy went to '{target}' ({user}@{host}), and \
         `stacker destroy` only tears down local deployments — it runs docker on this \
         machine.\n\n\
         The remote stack is still running. To tear it down:\n  \
         ssh{key_hint}{port_hint} {user}@{host} 'cd /home/trydirect/project && \
         docker compose -p project down'\n\n\
         Add --volumes to that command to drop its data as well. To destroy a local \
         deployment instead, switch targets first: stacker target local",
        target = lock.target,
    )))
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
        std::fs::write(dir.path().join("stacker.yml"), "name: demo\n").unwrap();
        let executor = MockExecutor::new();

        run_destroy(dir.path(), false, true, &executor).unwrap();

        let calls = executor.recorded_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "docker");
        assert!(calls[0].1.contains(&"down".to_string()));
        assert!(
            calls[0]
                .1
                .windows(2)
                .any(|w| w[0] == "-p" && w[1] == "demo"),
            "down must be scoped to the project: {:?}",
            calls[0].1
        );
    }

    /// The failure this guards: AstrBot's `deploy.cloud.ssh_key` referenced an
    /// undefined `${BASE_PATH}`, strict parsing failed, and the name fell back
    /// to the literal "stacker". `docker compose -p stacker down` matched
    /// nothing, exited 0, and destroy printed "✓ Stack destroyed successfully"
    /// three times in a row while the stack stayed up.
    #[test]
    fn an_unresolved_variable_in_an_inactive_target_does_not_derail_destroy() {
        let dir = setup_with_compose();
        std::fs::write(
            dir.path().join("stacker.yml"),
            "name: astrbot\nproject:\n  identity: astrbot\ndeploy:\n  target: local\n  \
             cloud:\n    provider: hetzner\n    ssh_key: ${BASE_PATH}/key\n",
        )
        .unwrap();
        let executor = MockExecutor::new();

        run_destroy(dir.path(), false, true, &executor).expect("destroy should run");

        let calls = executor.recorded_calls();
        assert!(
            calls[0]
                .1
                .windows(2)
                .any(|w| w[0] == "-p" && w[1] == "astrbot"),
            "the project's own name must be used, never a shared fallback: {:?}",
            calls[0].1
        );
    }

    /// A server deploy leaves a running remote stack and a lock that describes
    /// how to reach it. Reporting "No deployment found. Nothing to destroy."
    /// sent operators hunting for broken local state while the stack stayed up
    /// and the host collected orphaned containers.
    #[test]
    fn a_server_deployment_is_explained_rather_than_called_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let stacker_dir = dir.path().join(".stacker");
        std::fs::create_dir_all(&stacker_dir).unwrap();
        std::fs::write(
            dir.path().join("stacker.yml"),
            "name: demo\ndeploy:\n  target: server\n  server:\n    host: 46.224.127.228\n",
        )
        .unwrap();
        std::fs::write(
            stacker_dir.join("deployment-server.lock"),
            "target: server\nserver_ip: 46.224.127.228\nssh_user: root\nssh_port: 22\n\
             server_name: null\ndeployment_id: 417\nproject_id: null\ncloud_id: null\n\
             project_name: demo\nstacker_email: null\ndeployed_at: '2026-10-04T10:00:00Z'\n",
        )
        .unwrap();
        let executor = MockExecutor::new();

        let err = run_destroy(dir.path(), false, true, &executor)
            .expect_err("a remote deployment is out of this command's reach");
        let msg = err.to_string();

        assert!(
            !msg.contains("Nothing to destroy"),
            "the stack is running; saying there is nothing to destroy is false: {msg}"
        );
        assert!(
            msg.contains("46.224.127.228"),
            "the operator needs the host: {msg}"
        );
        assert!(
            msg.contains("docker compose"),
            "the operator needs the command that does work: {msg}"
        );
        assert!(
            executor.recorded_calls().is_empty(),
            "nothing may run locally for a remote deployment"
        );
    }

    /// Without any deployment at all, "nothing to destroy" is simply true.
    #[test]
    fn with_no_deployment_at_all_the_original_message_stands() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("stacker.yml"), "name: demo\n").unwrap();
        let executor = MockExecutor::new();

        let err =
            run_destroy(dir.path(), false, true, &executor).expect_err("nothing was deployed");
        assert!(err.to_string().contains("No deployment found"), "{err}");
    }

    /// With no config and nothing running, destroy must say so rather than
    /// run `down` against an invented project and report success.
    #[test]
    fn destroy_refuses_when_the_project_name_cannot_be_established() {
        let dir = setup_with_compose();
        let executor = MockExecutor::new();

        let err = run_destroy(dir.path(), false, true, &executor)
            .expect_err("a name that cannot be established must stop the teardown");
        let msg = err.to_string();
        assert!(
            msg.contains("project name"),
            "the error should say what could not be determined: {msg}"
        );
        assert!(
            executor
                .recorded_calls()
                .iter()
                .all(|(_, args)| !args.contains(&"down".to_string())),
            "nothing may be torn down under a guessed name"
        );
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
