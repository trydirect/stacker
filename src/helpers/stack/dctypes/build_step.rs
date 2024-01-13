use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum BuildStep {
    Simple(String),
    Advanced(dctypes::AdvancedBuildStep),
}
