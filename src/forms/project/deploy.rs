use crate::forms;
use crate::forms::{CloudForm, ServerForm};
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;

/// Docker registry credentials for pulling private images during deployment.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryForm {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_registry: Option<String>,
}

/// Validates that cloud deployments have required instance configuration
fn validate_cloud_instance_config(deploy: &Deploy) -> Result<(), serde_valid::validation::Error> {
    // Skip validation for "own" server deployments
    if deploy.cloud.provider == "own" {
        return Ok(());
    }

    let mut missing = Vec::new();

    if deploy.server.region.as_ref().map_or(true, |s| s.is_empty()) {
        missing.push("region");
    }
    if deploy.server.server.as_ref().map_or(true, |s| s.is_empty()) {
        missing.push("server");
    }
    if deploy.server.os.as_ref().map_or(true, |s| s.is_empty()) {
        missing.push("os");
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(serde_valid::validation::Error::Custom(format!(
            "Instance configuration incomplete. Missing: {}. Select datacenter, hardware, and OS before deploying.",
            missing.join(", ")
        )))
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[validate(custom(validate_cloud_instance_config))]
pub struct Deploy {
    #[validate]
    pub(crate) stack: Stack,
    #[validate]
    pub(crate) server: ServerForm,
    #[validate]
    pub(crate) cloud: CloudForm,
    /// Optional Docker registry credentials for pulling private images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) registry: Option<RegistryForm>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Stack {
    #[validate(min_length = 2)]
    #[validate(max_length = 255)]
    pub stack_code: Option<String>,
    pub vars: Option<Vec<forms::project::Var>>,
    pub integrated_features: Option<Vec<Value>>,
    pub extended_features: Option<Vec<Value>>,
    pub subscriptions: Option<Vec<String>>,
    pub form_app: Option<Vec<String>>,
}
