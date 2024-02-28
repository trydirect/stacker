use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes::*;
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
use serde_yaml::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Compose {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Services::is_empty")]
    pub services: Services,
    #[serde(default, skip_serializing_if = "TopLevelVolumes::is_empty")]
    pub volumes: TopLevelVolumes,
    #[serde(default, skip_serializing_if = "ComposeNetworks::is_empty")]
    pub networks: ComposeNetworks,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<Service>,
    #[cfg(feature = "indexmap")]
    #[serde(flatten, skip_serializing_if = "IndexMap::is_empty")]
    pub extensions: IndexMap<Extension, Value>,
    #[cfg(not(feature = "indexmap"))]
    #[serde(flatten, skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<Extension, Value>,
}
