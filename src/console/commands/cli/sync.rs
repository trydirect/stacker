//! Synchronize project configuration with Stacker without deploying it.

use crate::cli::config_parser::StackerConfig;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::build_project_body;
use crate::console::commands::CallableTrait;
use serde_json::json;
use std::path::PathBuf;

/// The summary line of a sync (BUGS.md #54): a sync with no compose on disk
/// touches only the project configuration and must say so — a plain
/// "✓ synchronized" over a missing compose told the operator the platform was
/// in step when the stored compose the agent writes before every deploy was
/// left untouched.
fn sync_summary_line(project_name: &str, project_id: i32, compose_present: bool) -> String {
    if compose_present {
        format!("✓ Project '{}' synchronized (id={})", project_name, project_id)
    } else {
        format!(
            "⚠ Project '{}' configuration synchronized (id={}) — no compose was present to sync",
            project_name, project_id
        )
    }
}

/// What a sync covers and what it does not. `sync` pushes the stacker.yml
/// configuration to the platform; the server-side compose — the one the agent
/// writes before every deploy — only changes on `stacker deploy`.
fn sync_scope_lines(compose_present: bool) -> Vec<String> {
    let mut lines = vec![
        "  Covered: project configuration from stacker.yml (services, environment, deploy fields)"
            .to_string(),
    ];
    if compose_present {
        lines.push(
            "  Not covered: the server-side compose — `stacker deploy` regenerates and pushes it"
                .to_string(),
        );
    } else {
        lines.push(
            "⚠ No compose found (.stacker/docker-compose.yml) — the server-side compose was not \
             updated."
                .to_string(),
        );
        lines.push(
            "  Run `stacker deploy` to generate and push the compose; `sync` never carries it."
                .to_string(),
        );
    }
    lines
}

/// `stacker sync [--verify] [--json] [--deployment <HASH>]`
pub struct SyncCommand {
    pub file: Option<PathBuf>,
    pub deployment: Option<String>,
    pub environment: Option<String>,
    pub verify: bool,
    pub json: bool,
}

impl SyncCommand {
    pub fn new(
        file: Option<PathBuf>,
        deployment: Option<String>,
        environment: Option<String>,
        verify: bool,
        json: bool,
    ) -> Self {
        Self {
            file,
            deployment,
            environment,
            verify,
            json,
        }
    }
}

impl CallableTrait for SyncCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = std::env::current_dir()?;
        let config_path = self
            .file
            .clone()
            .unwrap_or_else(|| PathBuf::from("stacker.yml"));
        let config_path = if config_path.is_absolute() {
            config_path
        } else {
            project_dir.join(config_path)
        };

        let config = StackerConfig::from_file(&config_path)?.with_resolved_deploy_target(None)?;
        let project_name = config
            .project
            .identity
            .clone()
            .filter(|identity| !identity.trim().is_empty())
            .unwrap_or_else(|| config.name.clone());
        let mut body = build_project_body(&config);

        if let Some(environment) = self
            .environment
            .as_deref()
            .or(config.deploy.environment.as_deref())
            .filter(|environment| !environment.trim().is_empty())
        {
            body["environment"] = json!(environment);
        }

        let ctx = CliRuntime::new("project sync")?;
        let result = ctx.block_on(async {
            let project = ctx
                .client
                .find_project_by_name(&project_name)
                .await?
                .ok_or_else(|| {
                    crate::cli::error::CliError::ConfigValidation(format!(
                        "Project '{}' was not found on the Stacker server. Run `stacker deploy` first.",
                        project_name
                    ))
                })?;

            if let Some(deployment_hash) = self.deployment.as_deref() {
                let deployments = ctx.client.list_deployments(Some(project.id), Some(100)).await?;
                if !deployments
                    .iter()
                    .any(|deployment| deployment.deployment_hash == deployment_hash)
                {
                    return Err(crate::cli::error::CliError::ConfigValidation(format!(
                        "Deployment '{}' was not found for project '{}'",
                        deployment_hash, project.name
                    )));
                }
            }

            let result = ctx.client.sync_project(project.id, body).await?;
            Ok::<_, crate::cli::error::CliError>((project, result))
        })?;

        let (project, result) = result;
        let status = result
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("synced");
        let selected_deployment = self.deployment.as_deref();
        // BUGS.md #54: with no compose on disk the run is a warning, not a
        // plain success — sync never carried the compose in the first place.
        let compose_present = crate::cli::local_compose::resolve_local_compose_path(&project_dir)
            .map(|path| path.exists())
            .unwrap_or(false);
        let scope_lines = sync_scope_lines(compose_present);

        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "operation": "project_sync",
                    "project_id": project.id,
                    "project_name": project.name,
                    "status": status,
                    "verify": self.verify,
                    "deployment": selected_deployment,
                    "deployment_created": false,
                    "server_contacted": false,
                    "containers_started": false,
                    "compose_present": compose_present,
                    "coverage": scope_lines,
                    "result": result,
                }))?
            );
        } else {
            println!(
                "{}",
                sync_summary_line(&project.name, project.id, compose_present)
            );
            for line in &scope_lines {
                println!("{}", line);
            }
            if self.verify {
                println!("  ✓ Stacker database acknowledged the synchronized configuration");
            }
            if let Some(deployment) = selected_deployment {
                println!("  Deployment context: {}", deployment);
            }
            println!("  Containers were not started or restarted");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // BUGS.md #54: `sync` reported success over a deleted compose — the
    // stored compose stayed stale and nothing said the sync never touches it.
    #[test]
    fn sync_summary_warns_when_no_compose_is_present() {
        let missing = sync_summary_line("demo", 7, false);
        assert!(missing.starts_with('⚠'), "{missing}");
        assert!(missing.contains("no compose"), "{missing}");

        let present = sync_summary_line("demo", 7, true);
        assert!(present.starts_with('✓'), "{present}");
    }

    #[test]
    fn sync_scope_states_what_it_covers_and_points_at_deploy_when_compose_is_missing() {
        let missing = sync_scope_lines(false);
        assert!(
            missing.iter().any(|line| line.contains("stacker deploy")),
            "{missing:?}"
        );
        assert!(
            missing.iter().any(|line| line.starts_with('⚠')),
            "{missing:?}"
        );
        assert!(
            missing.iter().any(|line| line.contains("Covered:")),
            "{missing:?}"
        );

        let present = sync_scope_lines(true);
        assert!(
            !present.iter().any(|line| line.starts_with('⚠')),
            "{present:?}"
        );
        assert!(
            present.iter().any(|line| line.contains("Not covered:")),
            "{present:?}"
        );
    }
}
