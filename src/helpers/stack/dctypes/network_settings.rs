use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct NetworkSettings {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub attachable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[cfg(feature = "indexmap")]
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub driver_opts: IndexMap<String, Option<SingleValue>>,
    #[cfg(not(feature = "indexmap"))]
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub driver_opts: HashMap<String, Option<SingleValue>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enable_ipv6: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub internal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<ComposeNetwork>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipam: Option<Ipam>,
    #[serde(default, skip_serializing_if = "Labels::is_empty")]
    pub labels: Labels,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
