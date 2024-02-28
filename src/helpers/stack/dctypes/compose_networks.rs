#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
#[cfg(not(feature = "indexmap"))]
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes::*;

#[cfg(feature = "indexmap")]
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComposeNetworks(pub IndexMap<String, MapOrEmpty<NetworkSettings>>);
#[cfg(not(feature = "indexmap"))]
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComposeNetworks(pub HashMap<String, MapOrEmpty<NetworkSettings>>);

impl ComposeNetworks {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
