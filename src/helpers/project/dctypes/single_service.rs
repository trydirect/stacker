use serde::{Deserialize, Serialize};
use crate::helpers::project::dctypes;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct SingleService {
    pub service: dctypes::Service,
}

