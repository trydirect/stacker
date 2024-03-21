use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use crate::forms::project::*;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Web {
    #[serde(flatten)]
    pub app: App,
    pub custom: Option<bool>,
}
