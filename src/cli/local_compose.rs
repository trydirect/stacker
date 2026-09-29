use std::path::{Path, PathBuf};

use crate::cli::config_parser::{DeployTarget, StackerConfig};
use crate::cli::deployment_lock::DeploymentLock;
use crate::cli::error::CliError;

const OUTPUT_DIR: &str = ".stacker";
const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// Resolve the Compose project name for a local project, the same way
/// `LocalDeploy` does: from `stacker.yml`'s `project.identity`/`name`,
/// sanitized (see `install_runner::local_compose_project_name`). Every
/// caller that runs `docker compose` against a project's `.stacker/`
/// compose file (deploy, destroy, status, ...) must pass this via `-p` —
/// without it, Compose falls back to the compose file's containing
/// directory basename, which is `.stacker` for every project, so every
/// project defaults to the same shared scope ("stacker"). Operating on
/// that shared scope from one project's directory can recreate, remove, or
/// misreport another, unrelated project's containers. See GH issue #235.
///
/// Falls back to the literal "stacker" only when `stacker.yml` is
/// missing/unparseable (e.g. deleted after a deploy) — there's no project
/// identity left to recover in that case, so this is a best-effort
/// default, not a guarantee of isolation.
pub fn resolve_local_compose_project_name(project_dir: &Path) -> String {
    let config_path = project_dir.join(DEFAULT_CONFIG_FILE);
    StackerConfig::from_file(&config_path)
        .ok()
        .map(|config| crate::cli::install_runner::local_compose_project_name(&config))
        .unwrap_or_else(|| "stacker".to_string())
}

pub fn resolve_local_compose_path(project_dir: &Path) -> Result<PathBuf, CliError> {
    let generated = project_dir.join(OUTPUT_DIR).join("docker-compose.yml");
    let config_path = project_dir.join(DEFAULT_CONFIG_FILE);
    let mut selected_non_local_target = false;

    if config_path.exists() {
        if let Ok(config) = StackerConfig::from_file(&config_path) {
            if let Ok(config) = config.with_resolved_deploy_target(None) {
                selected_non_local_target = config.deploy.target != DeployTarget::Local;

                if config.deploy.target == DeployTarget::Local {
                    if let Some(compose_file) = config.deploy.compose_file {
                        let resolved = if compose_file.is_absolute() {
                            compose_file
                        } else {
                            project_dir.join(compose_file)
                        };
                        if resolved.exists() {
                            return Ok(resolved);
                        }
                    }
                }
            }
        }
    }

    if selected_non_local_target {
        // An explicitly selected local target plus a generated compose file are
        // a stronger signal than the declared deploy target in stacker.yml:
        // `stacker deploy --target local` generates `.stacker/docker-compose.yml`
        // without rewriting stacker.yml. When `stacker target local` is active,
        // the local artifacts win.
        let active_is_local = DeploymentLock::read_active_target(project_dir)
            .ok()
            .flatten()
            .as_deref()
            == Some("local");
        if active_is_local && generated.exists() {
            return Ok(generated);
        }
        return Err(CliError::ConfigValidation(
            "The selected deploy target is not local, so no local docker-compose file is available."
                .to_string(),
        ));
    }

    if generated.exists() {
        // Even when a generated compose file exists, if the deployment lock
        // says the last deploy was to cloud/server, don't treat it as local.
        // An explicit `stacker target local` always wins over the lock.
        let active_is_local = DeploymentLock::read_active_target(project_dir)
            .ok()
            .flatten()
            .as_deref()
            == Some("local");
        if !active_is_local {
            if let Ok(Some(lock)) = DeploymentLock::load_active(project_dir) {
                if lock.target != "local" {
                    return Err(CliError::ConfigValidation(format!(
                        "This project was deployed to '{}'. Use 'stacker agent logs' or switch to the local target.",
                        lock.target
                    )));
                }
            }
        }
        return Ok(generated);
    }

    Err(CliError::ConfigValidation(
        "No deployment found. Run 'stacker deploy' first.".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_local_compose_path_prefers_configured_compose_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("docker/local")).unwrap();
        std::fs::create_dir_all(dir.path().join(".stacker")).unwrap();
        std::fs::write(
            dir.path().join("docker/local/compose.yml"),
            "services: {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join(".stacker/docker-compose.yml"),
            "services: {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("stacker.yml"),
            "name: demo\ndeploy:\n  target: local\n  compose_file: docker/local/compose.yml\n",
        )
        .unwrap();

        let resolved = resolve_local_compose_path(dir.path()).unwrap();
        assert_eq!(resolved, dir.path().join("docker/local/compose.yml"));
    }

    #[test]
    fn test_resolve_local_compose_path_falls_back_to_generated_compose() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".stacker")).unwrap();
        std::fs::write(
            dir.path().join(".stacker/docker-compose.yml"),
            "services: {}\n",
        )
        .unwrap();

        let resolved = resolve_local_compose_path(dir.path()).unwrap();
        assert_eq!(resolved, dir.path().join(".stacker/docker-compose.yml"));
    }

    #[test]
    fn test_resolve_local_compose_path_rejects_remote_default_target() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".stacker")).unwrap();
        std::fs::write(
            dir.path().join(".stacker/docker-compose.yml"),
            "services: {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("stacker.yml"),
            "name: demo\ndeploy:\n  default_target: prod\n  targets:\n    local:\n      compose_file: docker/local/compose.yml\n    prod:\n      server:\n        host: 10.0.0.8\n        user: deploy\n        ssh_key: ~/.ssh/id_ed25519\n",
        )
        .unwrap();

        assert!(resolve_local_compose_path(dir.path()).is_err());
    }

    #[test]
    fn test_active_local_target_overrides_remote_config_target() {
        // Regression: a local deploy generates `.stacker/docker-compose.yml`
        // without rewriting stacker.yml's declared target; `stacker target
        // local` must make the local artifacts reachable.
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".stacker")).unwrap();
        std::fs::write(
            dir.path().join(".stacker/docker-compose.yml"),
            "services: {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("stacker.yml"),
            "name: demo\ndeploy:\n  target: server\n  server:\n    host: 10.0.0.8\n",
        )
        .unwrap();
        DeploymentLock::write_active_target(dir.path(), "local").unwrap();

        let resolved = resolve_local_compose_path(dir.path()).unwrap();
        assert_eq!(resolved, dir.path().join(".stacker/docker-compose.yml"));
    }

    #[test]
    fn test_active_local_target_wins_with_two_locks() {
        // Regression (posthog repro): deployment-local.lock and
        // deployment-server.lock both present, stacker.yml target: server,
        // `.stacker/active-target` set to local via `stacker target local`.
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".stacker")).unwrap();
        std::fs::write(
            dir.path().join(".stacker/docker-compose.yml"),
            "services: {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("stacker.yml"),
            "name: posthog\ndeploy:\n  target: server\n",
        )
        .unwrap();
        DeploymentLock::for_local().save(dir.path()).unwrap();
        DeploymentLock::for_server(&crate::cli::config_parser::ServerConfig {
            host: "203.0.113.10".to_string(),
            user: "root".to_string(),
            ssh_key: None,
            port: 22,
        })
        .save(dir.path())
        .unwrap();
        DeploymentLock::write_active_target(dir.path(), "local").unwrap();

        let resolved = resolve_local_compose_path(dir.path()).unwrap();
        assert_eq!(resolved, dir.path().join(".stacker/docker-compose.yml"));
    }
}
