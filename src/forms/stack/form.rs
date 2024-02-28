use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;
use std::collections::HashMap;
use std::fmt;
use crate::models;
use crate::forms;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Stack { 
    // #[validate(min_length=2)]
    // #[validate(max_length=255)]
    #[serde(rename = "commonDomain")]
    pub common_domain: Option<String>,
    pub domain_list: Option<forms::stack::DomainList>,
    #[validate(min_length = 2)]
    #[validate(max_length = 255)]
    pub stack_code: Option<String>,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub region: String,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub zone: Option<String>,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub server: String,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub os: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub ssl: String,
    pub vars: Option<Vec<forms::stack::Var>>,
    pub integrated_features: Option<Vec<Value>>,
    pub extended_features: Option<Vec<Value>>,
    pub subscriptions: Option<Vec<String>>,
    pub form_app: Option<Vec<String>>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub disk_type: Option<String>,
    pub save_token: bool,
    #[validate(min_length = 10)]
    #[validate(max_length = 255)]
    pub cloud_token: String,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub provider: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub selected_plan: String,
    pub custom: forms::stack::Custom,
}

impl TryFrom<&models::Stack> for Stack {
    type Error = String;

    fn try_from(stack: &models::Stack) -> Result<Self, Self::Error> {
        serde_json::from_value::<Stack>(stack.body.clone()).map_err(|err| format!("{:?}", err))
    }
}

impl Stack {
    pub async fn is_readable_docker_image(&self) -> Result<bool, String> {
        let mut is_active = true;
        for app in &self.custom.web {
            if !app.app.docker_image.is_active().await? {
                is_active = false;
                break;
            }
        }

        // temporarily disabled
        // if let Some(service) = &self.custom.service {
        //     for app in service {
        //         if !app.app.docker_image.is_active().await? {
        //             is_active = false;
        //             break;
        //         }
        //     }
        // }
        //
        // if let Some(features) = &self.custom.feature {
        //     for app in features {
        //         if !app.app.docker_image.is_active().await? {
        //             is_active = false;
        //             break;
        //         }
        //     }
        // }
        Ok(is_active)
    }
}

