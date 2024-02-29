use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(deny_unknown_fields)]
pub struct IpamConfig {
    pub subnet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
}
