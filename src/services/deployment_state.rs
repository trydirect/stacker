use serde::Serialize;

use crate::{
    helpers::{
        compose_env_file_reference, extract_capabilities, has_capability, has_capability_value,
        remote_runtime_compose_path, remote_runtime_env_path, NPM_CREDENTIAL_SOURCE_KEY,
    },
    models::{Agent, Deployment, ProjectApp},
};

pub const DEPLOYMENT_STATE_SCHEMA_VERSION: &str = "v1alpha1";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeploymentState {
    pub schema_version: String,
    pub deployment: DeploymentStateDeployment,
    pub agent: DeploymentAgentState,
    pub runtime: DeploymentRuntimeState,
    pub apps: Vec<DeploymentAppState>,
    pub drift: DeploymentDriftState,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeploymentStateDeployment {
    pub id: i32,
    pub project_id: i32,
    pub deployment_hash: String,
    pub status: String,
    pub runtime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_command_status: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeploymentAgentState {
    pub status: String,
    pub online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
    pub capabilities: Vec<String>,
    pub features: DeploymentAgentFeatures,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct DeploymentAgentFeatures {
    pub kata_runtime: bool,
    pub compose: bool,
    pub backup: bool,
    pub pipes: bool,
    pub proxy_credentials_vault: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeploymentRuntimeState {
    pub compose_file: String,
    pub env_file: String,
    pub compose_env_file: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeploymentAppState {
    pub code: String,
    pub image: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_version: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_sync_version: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,
    pub needs_vault_sync: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeploymentDriftState {
    pub config_sync_pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_env_drift: Option<bool>,
}

impl DeploymentState {
    pub fn from_parts(deployment: &Deployment, agent: Option<&Agent>, apps: &[ProjectApp]) -> Self {
        let mut app_states = apps
            .iter()
            .map(DeploymentAppState::from)
            .collect::<Vec<_>>();
        app_states.sort_by(|left, right| left.code.cmp(&right.code));

        let config_sync_pending = app_states.iter().any(|app| app.needs_vault_sync);

        Self {
            schema_version: DEPLOYMENT_STATE_SCHEMA_VERSION.to_string(),
            deployment: DeploymentStateDeployment::from(deployment),
            agent: DeploymentAgentState::from(agent),
            runtime: DeploymentRuntimeState::default(),
            apps: app_states,
            drift: DeploymentDriftState {
                config_sync_pending,
                runtime_env_drift: None,
            },
        }
    }
}

impl From<&Deployment> for DeploymentStateDeployment {
    fn from(deployment: &Deployment) -> Self {
        Self {
            id: deployment.id,
            project_id: deployment.project_id,
            deployment_hash: deployment.deployment_hash.clone(),
            status: deployment.status.clone(),
            runtime: deployment.runtime.clone(),
            user_id: deployment.user_id.clone(),
            status_message: metadata_string(&deployment.metadata, "status_message"),
            last_command_status: metadata_string(&deployment.metadata, "last_command_status"),
            created_at: deployment.created_at,
            updated_at: deployment.updated_at,
        }
    }
}

impl From<Option<&Agent>> for DeploymentAgentState {
    fn from(agent: Option<&Agent>) -> Self {
        let Some(agent) = agent else {
            return Self {
                status: "offline".to_string(),
                online: false,
                agent_id: None,
                version: None,
                last_heartbeat: None,
                capabilities: Vec::new(),
                features: DeploymentAgentFeatures::default(),
            };
        };

        let capabilities = extract_capabilities(agent.capabilities.clone());
        let features = DeploymentAgentFeatures {
            kata_runtime: has_capability(&capabilities, "kata"),
            compose: has_capability(&capabilities, "compose"),
            backup: has_capability(&capabilities, "backup"),
            pipes: has_capability(&capabilities, "pipes"),
            proxy_credentials_vault: has_capability_value(
                &capabilities,
                NPM_CREDENTIAL_SOURCE_KEY,
                "vault",
            ),
        };

        Self {
            status: agent.status.clone(),
            online: agent.is_online(),
            agent_id: Some(agent.id.to_string()),
            version: agent.version.clone(),
            last_heartbeat: agent.last_heartbeat,
            capabilities,
            features,
        }
    }
}

impl Default for DeploymentRuntimeState {
    fn default() -> Self {
        Self {
            compose_file: remote_runtime_compose_path().to_string(),
            env_file: remote_runtime_env_path().to_string(),
            compose_env_file: compose_env_file_reference().to_string(),
        }
    }
}

impl From<&ProjectApp> for DeploymentAppState {
    fn from(app: &ProjectApp) -> Self {
        Self {
            code: app.code.clone(),
            image: app.image.clone(),
            enabled: app.is_enabled(),
            deployment_id: app.deployment_id,
            config_version: app.config_version,
            vault_sync_version: app.vault_sync_version,
            config_hash: app.config_hash.clone(),
            needs_vault_sync: app.needs_vault_sync(),
        }
    }
}

fn metadata_string(metadata: &serde_json::Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    use super::*;

    #[test]
    fn deployment_state_serialization_matches_online_fixture() {
        let deployment = sample_deployment();
        let agent = sample_agent();
        let app = synced_app();

        let state = DeploymentState::from_parts(&deployment, Some(&agent), &[app]);
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/ai/deployment_state.online.json"
        ))
        .unwrap();

        assert_eq!(serde_json::to_value(state).unwrap(), expected);
    }

    #[test]
    fn deployment_state_serialization_matches_offline_fixture() {
        let mut deployment = sample_deployment();
        deployment.status = "degraded".to_string();
        deployment.metadata = serde_json::json!({});
        let app = unsynced_app();

        let state = DeploymentState::from_parts(&deployment, None, &[app]);
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/ai/deployment_state.offline.json"
        ))
        .unwrap();

        assert_eq!(serde_json::to_value(state).unwrap(), expected);
    }

    #[test]
    fn deployment_state_omits_missing_optional_fields() {
        let mut deployment = sample_deployment();
        deployment.user_id = None;
        deployment.metadata = serde_json::json!({});

        let mut agent = sample_agent();
        agent.version = None;
        agent.last_heartbeat = None;

        let mut app = synced_app();
        app.deployment_id = None;
        app.config_hash = None;

        let state = DeploymentState::from_parts(&deployment, Some(&agent), &[app]);
        let serialized = serde_json::to_value(state).unwrap();

        assert!(serialized["deployment"].get("user_id").is_none());
        assert!(serialized["deployment"].get("status_message").is_none());
        assert!(serialized["deployment"]
            .get("last_command_status")
            .is_none());
        assert!(serialized["agent"].get("version").is_none());
        assert!(serialized["agent"].get("last_heartbeat").is_none());
        assert!(serialized["apps"][0].get("deployment_id").is_none());
        assert!(serialized["apps"][0].get("config_hash").is_none());
        assert!(serialized["drift"].get("runtime_env_drift").is_none());
    }

    #[test]
    fn deployment_state_marks_unsynced_apps_in_drift_summary() {
        let deployment = sample_deployment();
        let agent = sample_agent();

        let state = DeploymentState::from_parts(&deployment, Some(&agent), &[unsynced_app()]);

        assert!(state.drift.config_sync_pending);
        assert!(state.apps[0].needs_vault_sync);
    }

    fn sample_deployment() -> Deployment {
        Deployment {
            id: 77,
            project_id: 12,
            deployment_hash: "deployment_120254c6-598e-47a1-83ca-690840edd906".to_string(),
            user_id: Some("user_123".to_string()),
            deleted: Some(false),
            status: "deployed".to_string(),
            runtime: "runc".to_string(),
            metadata: serde_json::json!({
                "status_message": "Deployment complete",
                "last_command_status": "completed"
            }),
            last_seen_at: None,
            created_at: fixed_time("2026-05-14T09:00:00Z"),
            updated_at: fixed_time("2026-05-14T09:05:00Z"),
        }
    }

    fn sample_agent() -> Agent {
        Agent {
            id: Uuid::parse_str("36cf6fd2-6d76-4faf-9310-8f264c28fdb0").unwrap(),
            deployment_hash: "deployment_120254c6-598e-47a1-83ca-690840edd906".to_string(),
            capabilities: Some(serde_json::json!([
                "docker",
                "compose",
                "logs",
                "npm_credential_source=vault"
            ])),
            version: Some("0.42.0".to_string()),
            system_info: Some(serde_json::json!({
                "os": "linux"
            })),
            last_heartbeat: Some(fixed_time("2026-05-14T09:06:00Z")),
            status: "online".to_string(),
            created_at: fixed_time("2026-05-14T08:00:00Z"),
            updated_at: fixed_time("2026-05-14T09:06:00Z"),
        }
    }

    fn synced_app() -> ProjectApp {
        ProjectApp {
            id: 5,
            project_id: 12,
            code: "upload".to_string(),
            name: "Upload".to_string(),
            image: "optimum/syncopia-upload:latest".to_string(),
            environment: None,
            ports: None,
            volumes: None,
            domain: None,
            ssl_enabled: Some(false),
            resources: None,
            restart_policy: Some("unless-stopped".to_string()),
            command: None,
            entrypoint: None,
            networks: None,
            depends_on: None,
            healthcheck: None,
            labels: None,
            config_files: None,
            template_source: None,
            enabled: Some(true),
            deploy_order: Some(1),
            created_at: fixed_time("2026-05-14T08:30:00Z"),
            updated_at: fixed_time("2026-05-14T09:04:00Z"),
            config_version: Some(3),
            vault_synced_at: None,
            vault_sync_version: Some(3),
            config_hash: Some("hash-upload-v3".to_string()),
            parent_app_code: None,
            deployment_id: Some(77),
        }
    }

    fn unsynced_app() -> ProjectApp {
        let mut app = synced_app();
        app.config_version = Some(4);
        app.vault_sync_version = Some(3);
        app.config_hash = Some("hash-upload-v4".to_string());
        app
    }

    fn fixed_time(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .unwrap()
            .with_timezone(&Utc)
    }
}
