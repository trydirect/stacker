use crate::cli::credentials::{CredentialsManager, FileCredentialStore, StoredCredentials};
use crate::cli::deployment_lock::DeploymentLock;
use crate::cli::error::CliError;
use crate::cli::stacker_client::{StackerClient, DEFAULT_STACKER_URL};
use crate::console::commands::CallableTrait;
use crate::handoff::{DeploymentHandoffCredentials, DeploymentHandoffPayload, DeploymentHandoffProject};
use chrono::{DateTime, Utc};
use std::path::Path;

const DEFAULT_STACKER_YML: &str = "stacker.yml";

pub struct ConnectCommand {
    pub handoff: String,
}

impl ConnectCommand {
    pub fn new(handoff: String) -> Self {
        Self { handoff }
    }
}

impl CallableTrait for ConnectCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let token = extract_handoff_token(&self.handoff)?;
        let base_url = extract_handoff_base_url(&self.handoff);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;
        let payload = rt.block_on(StackerClient::resolve_handoff(&base_url, &token))?;

        hydrate_project_dir(std::env::current_dir()?.as_path(), &payload)?;
        eprintln!(
            "✓ Connected deployment {} to this directory",
            payload.deployment.hash
        );
        eprintln!("  You can now run: stacker status");
        Ok(())
    }
}

fn extract_handoff_base_url(input: &str) -> String {
    if let Ok(url) = reqwest::Url::parse(input) {
        let scheme = url.scheme();
        if let Some(host) = url.host_str() {
            if let Some(port) = url.port() {
                return format!("{}://{}:{}", scheme, host, port);
            }
            return format!("{}://{}", scheme, host);
        }
    }
    DEFAULT_STACKER_URL.to_string()
}

fn extract_handoff_token(input: &str) -> Result<String, CliError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CliError::ConfigValidation(
            "Handoff token or URL is required".to_string(),
        ));
    }

    if let Some(token) = trimmed.strip_prefix("stacker://handoff/") {
        return Ok(token.to_string());
    }

    if let Ok(url) = reqwest::Url::parse(trimmed) {
        if let Some(fragment) = url.fragment() {
            if !fragment.trim().is_empty() {
                return Ok(fragment.to_string());
            }
        }
        if let Some(last) = url
            .path_segments()
            .and_then(|segments| segments.filter(|segment| !segment.is_empty()).last())
        {
            if last != "handoff" {
                return Ok(last.to_string());
            }
        }
    }

    Ok(trimmed.to_string())
}

fn hydrate_project_dir(project_dir: &Path, payload: &DeploymentHandoffPayload) -> Result<(), CliError> {
    let lock: DeploymentLock = serde_json::from_value(payload.lockfile.clone()).map_err(|e| {
        CliError::ConfigValidation(format!("Invalid deployment lock in handoff payload: {}", e))
    })?;
    lock.save(project_dir)?;

    let stacker_yml_path = project_dir.join(DEFAULT_STACKER_YML);
    if !stacker_yml_path.exists() {
        let contents = payload
            .stacker_yml
            .clone()
            .unwrap_or_else(|| render_default_stacker_yml(&payload.project, &payload.deployment.hash));
        std::fs::write(&stacker_yml_path, contents).map_err(CliError::Io)?;
    }

    if let Some(credentials) = payload.credentials.as_ref() {
        save_handoff_credentials(credentials)?;
    }

    Ok(())
}

fn render_default_stacker_yml(project: &DeploymentHandoffProject, deployment_hash: &str) -> String {
    format!(
        "name: {}\nproject:\n  identity: {}\ndeploy:\n  target: cloud\n  deployment_hash: {}\n",
        yaml_string(&project.name),
        yaml_string(project.identity.as_deref().unwrap_or(&project.name)),
        yaml_string(deployment_hash)
    )
}

fn yaml_string(value: &str) -> String {
    serde_yaml::to_string(value)
        .map(|yaml| yaml.trim().to_string())
        .unwrap_or_else(|_| format!("{:?}", value))
}

fn save_handoff_credentials(credentials: &DeploymentHandoffCredentials) -> Result<(), CliError> {
    let manager = CredentialsManager::<FileCredentialStore>::with_default_store();
    let stored = StoredCredentials {
        access_token: credentials.access_token.clone(),
        refresh_token: None,
        token_type: credentials.token_type.clone(),
        expires_at: identity_expiry(credentials.expires_at),
        email: credentials.email.clone(),
        server_url: credentials.server_url.clone(),
        org: None,
        domain: None,
    };
    manager.save(&stored)
}

fn identity_expiry(expires_at: DateTime<Utc>) -> DateTime<Utc> {
    expires_at
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff::{DeploymentHandoffDeployment, DeploymentHandoffPayload};
    use chrono::Duration;
    use tempfile::TempDir;

    #[test]
    fn extracts_token_from_handoff_url_fragment() {
        let token =
            extract_handoff_token("https://stacker.try.direct/handoff#abc123-token").unwrap();
        assert_eq!(token, "abc123-token");
    }

    #[test]
    fn hydrates_project_dir_from_payload() {
        let temp_dir = TempDir::new().unwrap();
        let payload = DeploymentHandoffPayload {
            version: 1,
            expires_at: Utc::now() + Duration::minutes(5),
            project: DeploymentHandoffProject {
                id: 7,
                name: "demo".to_string(),
                identity: Some("demo".to_string()),
            },
            deployment: DeploymentHandoffDeployment {
                id: 12,
                hash: "dep-123".to_string(),
                target: "cloud".to_string(),
                status: "running".to_string(),
            },
            server: None,
            cloud: None,
            lockfile: serde_json::json!({
                "target": "cloud",
                "server_ip": "127.0.0.1",
                "ssh_user": "root",
                "ssh_port": 22,
                "server_name": "demo",
                "deployment_id": 12,
                "project_id": 7,
                "cloud_id": 9,
                "project_name": "demo",
                "deployed_at": "2026-04-12T10:00:00Z"
            }),
            stacker_yml: Some("name: demo\n".to_string()),
            agent: None,
            credentials: Some(DeploymentHandoffCredentials {
                access_token: "token-1".to_string(),
                token_type: "Bearer".to_string(),
                expires_at: Utc::now() + Duration::minutes(5),
                email: Some("demo@example.com".to_string()),
                server_url: Some("https://stacker.try.direct".to_string()),
            }),
        };

        hydrate_project_dir(temp_dir.path(), &payload).unwrap();

        assert!(temp_dir.path().join("stacker.yml").exists());
        let lock = DeploymentLock::load(temp_dir.path()).unwrap().unwrap();
        assert_eq!(lock.deployment_id, Some(12));
        assert_eq!(lock.project_name.as_deref(), Some("demo"));
    }
}
