use serde::{Deserialize, Serialize};
use crate::forms::stack::*;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Web {
    #[serde(flatten)]
    pub app: App,
    pub custom: Option<bool>,
    pub main: bool,
}
