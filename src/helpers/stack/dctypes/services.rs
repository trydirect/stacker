use serde::{Deserialize, Serialize};
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
#[cfg(not(feature = "indexmap"))]
use std::collections::HashMap;
use crate::helpers::stack::dctypes;

#[cfg(feature = "indexmap")]
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct Services(pub IndexMap<String, Option<dctypes::Service>>);
#[cfg(not(feature = "indexmap"))]
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct Services(pub HashMap<String, Option<dctypes::Service>>);

impl Services {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
