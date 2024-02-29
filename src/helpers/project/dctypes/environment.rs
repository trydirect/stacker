use serde::{Deserialize, Serialize};
use serde_yaml::Value;
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
use crate::helpers::project::dctypes;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Environment {
    List(Vec<String>),
    #[cfg(feature = "indexmap")]
    KvPair(IndexMap<String, Option<dctypes::SingleValue>>),
    #[cfg(not(feature = "indexmap"))]
    KvPair(HashMap<String, Option<dctypes::SingleValue>>),
}

impl Default for Environment {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

impl Environment {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::List(v) => v.is_empty(),
            Self::KvPair(m) => m.is_empty(),
        }
    }
}
