use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes;
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
use serde_yaml::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Compose {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "dctypes::Services::is_empty")]
    pub services: dctypes::Services,
    #[serde(default, skip_serializing_if = "dctypes::TopLevelVolumes::is_empty")]
    pub volumes: dctypes::TopLevelVolumes,
    #[serde(default, skip_serializing_if = "dctypes::ComposeNetworks::is_empty")]
    pub networks: dctypes::ComposeNetworks,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<dctypes::Service>,
    #[cfg(feature = "indexmap")]
    #[serde(flatten, skip_serializing_if = "IndexMap::is_empty")]
    pub extensions: IndexMap<dctypes::Extension, Value>,
    #[cfg(not(feature = "indexmap"))]
    #[serde(flatten, skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<dctypes::Extension, Value>,
}
