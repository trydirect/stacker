use serde::{Deserialize, Serialize};
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
#[cfg(not(feature = "indexmap"))]
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum BuildArgs {
    Simple(String),
    List(Vec<String>),
    #[cfg(feature = "indexmap")]
    KvPair(IndexMap<String, String>),
    #[cfg(not(feature = "indexmap"))]
    KvPair(HashMap<String, String>),
}
