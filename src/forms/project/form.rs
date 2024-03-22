use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;
use actix_web::Error;
use actix_web::web::Bytes;
use crate::models;
use crate::forms;
use crate::helpers::JsonResponse;
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
        let mut is_active = true;
        for app in &self.custom.web {
            if !app.app.docker_image.is_active().await? {
                is_active = false;
                break;
            }
        }

        if let Some(service) = &self.custom.service {
            for app in service {
                if !app.app.docker_image.is_active().await? {
                    is_active = false;
                    break;
                }
            }
        }

        if let Some(features) = &self.custom.feature {
            for app in features {
                if !app.app.docker_image.is_active().await? {
                    is_active = false;
                    break;
                }
            }
        }
        Ok(is_active)
    }
}
