//! `stacker notify` — manage notification channels (WS-N N4).
//!
//! Thin client over the `/api/v1/notifications/*` REST API. Secrets are
//! write-only: they go to Vault on the server and are never echoed back.

use crate::cli::error::CliError;
use crate::cli::stacker_client::StackerClient;
use crate::console::commands::cli::status::resolve_stacker_base_url;
use crate::console::commands::CallableTrait;

pub enum NotifyAction {
    Add {
        name: String,
        channel_type: String,
        project: Option<i32>,
        secret: String,
        events: Option<String>,
    },
    Test {
        id: i32,
    },
    List {
        project: Option<i32>,
    },
}

pub struct NotifyCommand {
    action: NotifyAction,
}

impl NotifyCommand {
    pub fn add(
        name: String,
        channel_type: String,
        project: Option<i32>,
        secret: String,
        events: Option<String>,
    ) -> Self {
        Self {
            action: NotifyAction::Add {
                name,
                channel_type,
                project,
                secret,
                events,
            },
        }
    }

    pub fn test(id: i32) -> Self {
        Self {
            action: NotifyAction::Test { id },
        }
    }

    pub fn list(project: Option<i32>) -> Self {
        Self {
            action: NotifyAction::List { project },
        }
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let cred_manager = crate::cli::credentials::CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("notify")?;
        let base_url = resolve_stacker_base_url(&creds);
        let client = StackerClient::new(&base_url, &creds.access_token);

        match &self.action {
            NotifyAction::Add {
                name,
                channel_type,
                project,
                secret,
                events,
            } => {
                let secret_value: serde_json::Value = serde_json::from_str(secret)
                    .map_err(|e| CliError::ConfigValidation(format!("--secret must be JSON: {}", e)))?;
                let mut body = serde_json::json!({
                    "name": name,
                    "channelType": channel_type,
                    "secret": secret_value,
                });
                if let Some(project) = project {
                    body["projectId"] = serde_json::json!(project);
                }
                if let Some(events) = events {
                    let list: Vec<String> =
                        events.split(',').map(|s| s.trim().to_string()).collect();
                    body["events"] = serde_json::json!(list);
                }
                let response = client.create_notification_channel(&body).await?;
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    return Err(Box::new(CliError::ConfigValidation(format!(
                        "Server rejected the channel ({}): {}",
                        status, text
                    ))));
                }
                println!("Channel created: {}", text);
            }
            NotifyAction::Test { id } => {
                let response = client.test_notification_channel(*id).await?;
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    return Err(Box::new(CliError::ConfigValidation(format!(
                        "Server rejected the test ({}): {}",
                        status, text
                    ))));
                }
                println!("{}", text);
            }
            NotifyAction::List { project } => {
                let response = client.list_notification_channels(*project).await?;
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    return Err(Box::new(CliError::ConfigValidation(format!(
                        "Server rejected the listing ({}): {}",
                        status, text
                    ))));
                }
                println!("{}", text);
            }
        }
        Ok(())
    }
}

impl CallableTrait for NotifyCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::ConfigValidation(format!("Failed to initialize runtime: {}", e)))?;
        rt.block_on(self.run())
    }
}
