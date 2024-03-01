use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;
use std::collections::HashMap;
use std::fmt;
use crate::models;
use crate::forms;


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct ProjectForm {
    // #[validate(min_length=2)]
    // #[validate(max_length=255)]
    #[serde(rename = "commonDomain")]
    pub common_domain: Option<String>,
    pub domain_list: Option<forms::project::DomainList>,
    #[validate(min_length = 2)]
    #[validate(max_length = 255)]
    pub stack_code: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub ssl: String,
    pub vars: Option<Vec<forms::project::Var>>,
    pub integrated_features: Option<Vec<Value>>,
    pub extended_features: Option<Vec<Value>>,
    pub subscriptions: Option<Vec<String>>,
    pub form_app: Option<Vec<String>>,
    pub custom: forms::project::Custom,
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

