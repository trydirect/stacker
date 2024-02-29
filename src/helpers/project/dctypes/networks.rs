use serde::{Deserialize, Serialize};
use crate::helpers::project::dctypes;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum Networks {
    Simple(Vec<String>),
    Advanced(dctypes::AdvancedNetworks),
}

impl Default for Networks {
    fn default() -> Self {
        Self::Simple(Vec::new())
    }
}

impl Networks {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Simple(n) => n.is_empty(),
            Self::Advanced(n) => n.0.is_empty(),
        }
    }
}
