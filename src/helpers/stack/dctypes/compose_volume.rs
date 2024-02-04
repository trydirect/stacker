use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes::*;
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
#[cfg(not(feature = "indexmap"))]
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComposeVolume {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[cfg(feature = "indexmap")]
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub driver_opts: IndexMap<String, Option<SingleValue>>,
    #[cfg(not(feature = "indexmap"))]
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub driver_opts: HashMap<String, Option<SingleValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<ExternalVolume>,
    #[serde(default, skip_serializing_if = "Labels::is_empty")]
    pub labels: Labels,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

