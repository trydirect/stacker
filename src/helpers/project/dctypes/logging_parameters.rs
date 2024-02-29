use serde::{Deserialize, Serialize};
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
use crate::helpers::project::dctypes;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LoggingParameters {
    pub driver: String,
    #[cfg(feature = "indexmap")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<IndexMap<String, dctypes::SingleValue>>,
    #[cfg(not(feature = "indexmap"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<HashMap<String, dctypes::SingleValue>>,
}
