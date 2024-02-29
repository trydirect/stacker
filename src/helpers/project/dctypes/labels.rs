use serde::{Deserialize, Serialize};
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
#[cfg(not(feature = "indexmap"))]
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum Labels {
    List(Vec<String>),
    #[cfg(feature = "indexmap")]
    Map(IndexMap<String, String>),
    #[cfg(not(feature = "indexmap"))]
    Map(HashMap<String, String>),
}

impl Default for Labels {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

impl Labels {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::List(v) => v.is_empty(),
            Self::Map(m) => m.is_empty(),
        }
    }
}

