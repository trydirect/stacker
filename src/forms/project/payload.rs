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
    #[serde(flatten)]
    pub cloud: Option<forms::Cloud>,
    #[serde(flatten)]
    pub server: Option<forms::Server>,
    #[serde(flatten)]
    pub stack: forms::project::Stack,
    pub custom: forms::project::Custom,
    pub docker_compose: Option<Vec<u8>>,
}

impl TryFrom<&models::Project> for Payload {
    type Error = String;

    fn try_from(project: &models::Project) -> Result<Self, Self::Error> {
        // tracing::debug!("project body: {:?}", project.body.clone());
        let mut project_data = serde_json::from_value::<Payload>(project.body.clone())
            .map_err(|err| {
                format!("{:?}", err)
            })?;

        project_data.id = Some(project.id.clone());

        Ok(project_data)
    }
}