use serde::{Deserialize, Serialize};
use crate::forms::stack::*;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    // #[serde(rename(deserialize = "sharedPorts"))]
    // #[serde(rename(serialize = "shared_ports"))]
    // #[serde(alias = "shared_ports")]
    // pub shared_ports: Option<Vec<Port>>,
    #[serde(flatten)]
    pub app: App,
}