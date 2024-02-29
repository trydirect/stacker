use serde::{Deserialize, Serialize};
use crate::helpers::project::dctypes;
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SysCtls {
    List(Vec<String>),
    #[cfg(feature = "indexmap")]
    Map(IndexMap<String, Option<dctypes::SingleValue>>),
    #[cfg(not(feature = "indexmap"))]
    Map(HashMap<String, Option<dctypes::SingleValue>>),
}

impl Default for SysCtls {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

impl SysCtls {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::List(v) => v.is_empty(),
            Self::Map(m) => m.is_empty(),
        }
    }
}

