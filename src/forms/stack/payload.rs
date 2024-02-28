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
    pub domain_list: Option<forms::stack::DomainList>,
    pub region: String,
    pub zone: Option<String>,
    pub server: String,
    pub os: String,
    pub ssl: String,
    pub vars: Option<Vec<forms::stack::Var>>,
    #[serde(rename = "integrated_features")]
    pub integrated_features: Option<Vec<Value>>,
    #[serde(rename = "extended_features")]
    pub extended_features: Option<Vec<Value>>,
    pub subscriptions: Option<Vec<String>>,
    pub form_app: Option<Vec<String>>,
    pub disk_type: Option<String>,
    pub save_token: bool,
    pub cloud_token: Option<String>,
    pub cloud_key: Option<String>,
    pub cloud_secret: Option<String>,
    pub provider: String,
    pub stack_code: String,
    #[serde(rename = "selected_plan")]
    pub selected_plan: String,
    pub custom: forms::stack::Custom,
    pub docker_compose: Option<Vec<u8>>,
}

impl TryFrom<&models::Stack> for Payload {
    type Error = String;

    fn try_from(stack: &models::Stack) -> Result<Self, Self::Error> {
        let mut stack_data = serde_json::from_value::<Payload>(stack.body.clone()).map_err(|err| {
            format!("{:?}", err)
        })?;

        stack_data.id = Some(stack.id.clone());
        stack_data.stack_code = stack_data.custom.custom_stack_code.clone();

        Ok(stack_data)
    }
}
