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
