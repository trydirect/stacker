use std::convert::TryFrom;
use crate::models;
use crate::forms;
use serde_json::Value;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Payload {
    pub(crate) id: Option<i32>,
    pub(crate) user_token: Option<String>,
    pub(crate) user_email: Option<String>,
    #[serde(rename = "commonDomain")]
    pub common_domain: String,
    pub domain_list: Option<forms::project::DomainList>,
    #[serde(flatten)]
    pub server: models::Server,
    pub ssl: String,
    pub vars: Option<Vec<forms::project::Var>>,
    #[serde(rename = "integrated_features")]
    pub integrated_features: Option<Vec<Value>>,
    #[serde(rename = "extended_features")]
    pub extended_features: Option<Vec<Value>>,
    pub subscriptions: Option<Vec<String>>,
    pub form_app: Option<Vec<String>>,
    pub disk_type: Option<String>,
    #[serde(flatten)]
    pub cloud: models::Cloud,
    pub stack_code: String,
    pub custom: forms::project::Custom,
    pub docker_compose: Option<Vec<u8>>,
}

impl TryFrom<&models::Project> for Payload {
    type Error = String;

    fn try_from(project: &models::Project) -> Result<Self, Self::Error> {
        let mut project_data = serde_json::from_value::<Payload>(project.body.clone()).map_err(|err| {
            format!("{:?}", err)
        })?;

        project_data.id = Some(project.id.clone());
        project_data.stack_code = project_data.custom.custom_stack_code.clone();

        Ok(project_data)
    }
}
