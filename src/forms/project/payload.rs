use crate::forms;
use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::convert::TryFrom;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Payload {
    pub(crate) id: Option<i32>,
    pub(crate) project_id: Option<i32>,
    pub(crate) deployment_hash: Option<String>,
    pub(crate) user_token: Option<String>,
    pub(crate) user_email: Option<String>,
    #[serde(flatten)]
    pub cloud: Option<forms::CloudForm>,
    #[serde(flatten)]
    pub server: Option<forms::ServerForm>,
    #[serde(flatten)]
    pub stack: forms::project::Stack,
    pub custom: forms::project::Custom,
    pub docker_compose: Option<Vec<u8>>,
    /// Docker registry credentials for pulling private images on the target server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<forms::project::RegistryForm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_artifact_bundle: Option<serde_json::Value>,
}

impl TryFrom<&models::Project> for Payload {
    type Error = String;

    fn try_from(project: &models::Project) -> Result<Self, Self::Error> {
        // tracing::debug!("project metadata: {:?}", project.metadata.clone());
        let mut project_data = serde_json::from_value::<Payload>(project.metadata.clone())
            .map_err(|err| format!("{:?}", err))?;
        project_data.project_id = Some(project.id);

        Ok(project_data)
    }
}

#[cfg(test)]
mod tests {
    use super::Payload;
    use crate::models;
    use serde_json::json;

    #[test]
    fn payload_try_from_preserves_runtime_artifact_fields() {
        let project = models::Project::new(
            "user-1".to_string(),
            "runtime-artifacts".to_string(),
            json!({
                "stack": {
                    "stack_code": "runtime-artifacts"
                },
                "custom": {
                    "web": [],
                    "custom_stack_code": "runtime-artifacts",
                    "marketplace_config_files": [
                        {"path": "config/app.env", "content": "APP_ENV=prod"}
                    ],
                    "marketplace_assets": [
                        {"filename": "runtime-bundle.tgz", "key": "templates/1/runtime-bundle.tgz", "sha256": "abc123", "size": 12, "content_type": "application/gzip", "decompress": true}
                    ],
                    "marketplace_seed_jobs": [
                        {"name": "seed-admin"}
                    ],
                    "marketplace_post_deploy_hooks": [
                        {"name": "notify"}
                    ]
                },
                "runtime_artifact_bundle": {
                    "filename": "runtime-bundle.tgz",
                    "download_url": "https://objects.trydirect.test/runtime-bundle.tgz",
                    "seed_jobs_execution": "deferred",
                    "post_deploy_execution": "deferred"
                }
            }),
            json!({}),
        );

        let payload = Payload::try_from(&project).expect("payload should deserialize");
        let custom = serde_json::to_value(&payload.custom).expect("serialize custom");

        assert_eq!(
            custom["marketplace_config_files"][0]["path"],
            json!("config/app.env")
        );
        assert_eq!(
            custom["marketplace_assets"][0]["filename"],
            json!("runtime-bundle.tgz")
        );
        assert_eq!(
            custom["marketplace_seed_jobs"][0]["name"],
            json!("seed-admin")
        );
        assert_eq!(
            custom["marketplace_post_deploy_hooks"][0]["name"],
            json!("notify")
        );
        assert_eq!(
            payload
                .runtime_artifact_bundle
                .expect("runtime bundle should exist")["download_url"],
            json!("https://objects.trydirect.test/runtime-bundle.tgz")
        );
    }
}
