use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use crate::models;
use crate::forms;
use std::str;


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct ProjectForm {
    pub custom: forms::project::Custom
}

impl TryFrom<&models::Project> for ProjectForm {
    type Error = String;

    fn try_from(project: &models::Project) -> Result<Self, Self::Error> {
        serde_json::from_value::<ProjectForm>(project.body.clone()).map_err(|err| format!("{:?}", err))
    }
}

impl ProjectForm {
    pub async fn is_readable_docker_image(&self) -> Result<bool, String> {
        for app in &self.custom.web {
            if !app.app.docker_image.is_active().await? {
                return Ok(false);
            }
        }

        if let Some(service) = &self.custom.service {
            for app in service {
                if !app.app.docker_image.is_active().await? {
                    return Ok(false);
                }
            }
        }

        if let Some(features) = &self.custom.feature {
            for app in features {
                if !app.app.docker_image.is_active().await? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}