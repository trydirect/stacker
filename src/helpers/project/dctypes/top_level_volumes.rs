use serde::{Deserialize, Serialize};
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
#[cfg(not(feature = "indexmap"))]
use std::collections::HashMap;
use crate::helpers::project::dctypes::*;

#[cfg(feature = "indexmap")]
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct TopLevelVolumes(pub IndexMap<String, MapOrEmpty<ComposeVolume>>);
#[cfg(not(feature = "indexmap"))]
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct TopLevelVolumes(pub HashMap<String, MapOrEmpty<ComposeVolume>>);

impl TopLevelVolumes {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
