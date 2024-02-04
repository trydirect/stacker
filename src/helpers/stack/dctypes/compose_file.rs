use serde::{Deserialize, Serialize};
use crate::helpers::stack::dctypes;

#[cfg(feature = "indexmap")]
use indexmap::IndexMap;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ComposeFile {
    V2Plus(dctypes::Compose),
    #[cfg(feature = "indexmap")]
    V1(IndexMap<String, dctypes::Service>),
    #[cfg(not(feature = "indexmap"))]
    V1(HashMap<String, Service>),
    Single(dctypes::SingleService),
}

