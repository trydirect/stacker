use serde::{Deserialize, Serialize};
use crate::forms::project::*;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Volumes {
    volumes: Vec<Volume>,
}
