use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum Tmpfs {
    Simple(String),
    List(Vec<String>),
}

