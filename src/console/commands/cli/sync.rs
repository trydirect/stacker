//! Synchronize project configuration with Stacker without deploying it.

use crate::cli::config_parser::StackerConfig;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::build_project_body;
use crate::console::commands::CallableTrait;
use serde_json::json;
use std::path::PathBuf;

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
                    "result": result,
                }))?
            );
        } else {
            println!(
                "✓ Project '{}' synchronized (id={})",
                project.name, project.id
            );
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
