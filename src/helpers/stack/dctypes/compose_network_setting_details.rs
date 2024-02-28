use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(deny_unknown_fields)]
pub struct ComposeNetworkSettingDetails {
    pub name: String,
}
