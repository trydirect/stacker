//! `stacker backup` CLI (WS-1 1.5): list jobs/destinations/runs and trigger a
//! backup run.

use crate::cli::error::CliError;
use crate::cli::stacker_client::StackerClient;
use crate::console::commands::cli::status::resolve_stacker_base_url;
use crate::console::commands::CallableTrait;

#[derive(Debug, Clone)]
pub enum BackupAction {
    /// List backup jobs (optionally for a project).
    List { project: Option<i32> },
    /// Trigger a backup run for a job.
    Run { id: i32 },
    /// List backup destinations.
    Destinations,
    /// List runs for a job.
    Runs { job: i32 },
    /// Restore a specific run's archive.
    Restore { run: i32 },
}

#[derive(Debug, Clone)]
pub struct BackupCommand {
    action: BackupAction,
}

impl BackupCommand {
    pub fn list(project: Option<i32>) -> Self {
        Self {
            action: BackupAction::List { project },
        }
    }

    pub fn trigger(id: i32) -> Self {
        Self {
            action: BackupAction::Run { id },
        }
    }

    pub fn destinations() -> Self {
        Self {
            action: BackupAction::Destinations,
        }
    }

    pub fn runs(job: i32) -> Self {
        Self {
            action: BackupAction::Runs { job },
        }
    }

    pub fn restore(run: i32) -> Self {
        Self {
            action: BackupAction::Restore { run },
        }
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let cred_manager = crate::cli::credentials::CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("backup")?;
        let base_url = resolve_stacker_base_url(&creds);
        let client = StackerClient::new(&base_url, &creds.access_token);

        match &self.action {
            BackupAction::List { project } => {
                let response = client.list_backup_jobs(*project).await?;
                print_result(response, "Server rejected the listing").await?;
            }
            BackupAction::Run { id } => {
                let response = client.trigger_backup_run(*id).await?;
                print_result(response, "Server rejected the backup trigger").await?;
            }
            BackupAction::Destinations => {
                let response = client.list_backup_destinations().await?;
                print_result(response, "Server rejected the listing").await?;
            }
            BackupAction::Runs { job } => {
                let response = client.list_backup_runs(*job).await?;
                print_result(response, "Server rejected the listing").await?;
            }
            BackupAction::Restore { run } => {
                let response = client.trigger_backup_restore(*run).await?;
                print_result(response, "Server rejected the restore trigger").await?;
            }
        }
        Ok(())
    }
}

async fn print_result(
    response: reqwest::Response,
    error_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(Box::new(CliError::ConfigValidation(format!(
            "{} ({}): {}",
            error_prefix, status, text
        ))));
    }
    // Pretty-print the JSON payload when possible.
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap_or(text)),
        Err(_) => println!("{}", text),
    }
    Ok(())
}

impl CallableTrait for BackupCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                CliError::ConfigValidation(format!("Failed to initialize runtime: {}", e))
            })?;
        rt.block_on(self.run())
    }
}
