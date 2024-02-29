use serde::{Deserialize, Serialize};
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
use crate::helpers::project::dctypes;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum DependsOnOptions {
    Simple(Vec<String>),
    #[cfg(feature = "indexmap")]
    Conditional(IndexMap<String, dctypes::DependsCondition>),
    #[cfg(not(feature = "indexmap"))]
    Conditional(HashMap<String, dctypes::DependsCondition>),
}

impl Default for DependsOnOptions {
    fn default() -> Self {
        Self::Simple(Vec::new())
    }
}

impl DependsOnOptions {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Simple(v) => v.is_empty(),
            Self::Conditional(m) => m.is_empty(),
        }
    }
}

