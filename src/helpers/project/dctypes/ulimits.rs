use serde::{Deserialize, Serialize};
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
#[cfg(not(feature = "indexmap"))]
use std::collections::HashMap;
use crate::helpers::project::dctypes;

#[cfg(feature = "indexmap")]
#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Ulimits(pub IndexMap<String, dctypes::Ulimit>);
#[cfg(not(feature = "indexmap"))]
#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Ulimits(pub HashMap<String, dctypes::Ulimit>);

impl Ulimits {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

