use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes::*;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(untagged)]
pub enum ComposeNetwork {
    Detailed(ComposeNetworkSettingDetails),
    Bool(bool),
}
