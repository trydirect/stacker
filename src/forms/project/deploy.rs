use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;
use crate::forms;
use crate::forms::{Cloud, Server};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Deploy {
    #[validate]
    pub(crate) stack: Stack,
    #[validate]
    pub(crate) server: Server,
    #[validate]
    pub(crate) cloud: Cloud,
    // pub user_id: Option<String>,
    // pub project_id: Option<i32>
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