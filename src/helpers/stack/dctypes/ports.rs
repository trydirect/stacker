use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Ports {
    Short(Vec<String>),
    Long(Vec<dctypes::Port>),
}

impl Default for Ports {
    fn default() -> Self {
        Self::Short(Vec::default())
    }
}

impl Ports {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Short(v) => v.is_empty(),
            Self::Long(v) => v.is_empty(),
        }
    }
}

