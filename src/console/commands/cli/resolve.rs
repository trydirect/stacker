use crate::cli::config_parser::{DeployTarget, StackerConfig};
use crate::cli::credentials::CredentialsManager;
use crate::cli::error::CliError;
use crate::cli::stacker_client::{self, StackerClient};
use crate::console::commands::CallableTrait;

const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// `stacker resolve [--confirm]`
///
/// Force-complete a stuck deployment (paused or error → completed).
/// This unblocks operations like `add-app` that require a completed deployment.
pub struct ResolveCommand {
    pub confirm: bool,
}

impl ResolveCommand {
    pub fn new(confirm: bool) -> Self {
        Self { confirm }
    }
}

impl CallableTrait for ResolveCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.confirm {
            return Err(Box::new(CliError::ConfigValidation(
                "Resolve requires --confirm (-y) flag. This will mark a paused/error \
                 deployment as completed."
                    .to_string(),
            )));
        }

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

        let project_name = config
            .project
            .identity
            .clone()
            .unwrap_or_else(|| config.name.clone());

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("resolve")?;

        let base_url = stacker_client::DEFAULT_STACKER_URL.to_string();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::DeployFailed {
                target: DeployTarget::Cloud,
                reason: format!("Failed to initialize async runtime: {}", e),
            })?;

        rt.block_on(async move {
            let client = StackerClient::new(&base_url, &creds.access_token);

            // Find project
            let project = client.find_project_by_name(&project_name).await?;
            let project = project.ok_or_else(|| CliError::DeployFailed {
                target: DeployTarget::Cloud,
                reason: format!(
                    "Project '{}' not found on server.",
                    project_name
                ),
            })?;

            // Get latest deployment
            let status = client
                .get_deployment_status_by_project(project.id)
                .await?;

            let info = status.ok_or_else(|| CliError::DeployFailed {
                target: DeployTarget::Cloud,
                reason: format!(
                    "No deployments found for project '{}'.",
                    project_name
                ),
            })?;

            let allowed = ["paused", "error"];
            if !allowed.contains(&info.status.as_str()) {
                return Err(CliError::DeployFailed {
                    target: DeployTarget::Cloud,
                    reason: format!(
                        "Deployment #{} has status '{}'. Only paused or error deployments can be resolved.",
                        info.id, info.status
                    ),
                });
            }

            eprintln!(
                "Resolving deployment #{} (current status: '{}')...",
                info.id, info.status
            );

            let updated = client.force_complete_deployment(info.id).await?;

            eprintln!(
                "✓ Deployment #{} status changed to '{}'",
                updated.id, updated.status
            );

            Ok::<(), CliError>(())
        })?;

        Ok(())
    }
}
