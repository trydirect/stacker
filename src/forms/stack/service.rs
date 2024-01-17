use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use crate::forms::stack::*;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Service {
    // #[serde(rename(deserialize = "sharedPorts"))]
    // #[serde(rename(serialize = "shared_ports"))]
    // #[serde(alias = "shared_ports")]
    // pub shared_ports: Option<Vec<Port>>,
    #[serde(flatten)]
    pub(crate) app: App,
}
