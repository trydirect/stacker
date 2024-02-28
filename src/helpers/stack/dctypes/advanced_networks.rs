use serde::{Deserialize, Serialize};
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
#[cfg(not(feature = "indexmap"))]
use std::collections::HashMap;
use crate::helpers::stack::dctypes::*;

#[cfg(feature = "indexmap")]
#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct AdvancedNetworks(pub IndexMap<String, MapOrEmpty<AdvancedNetworkSettings>>);
#[cfg(not(feature = "indexmap"))]
#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct AdvancedNetworks(pub HashMap<String, MapOrEmpty<AdvancedNetworkSettings>>);
