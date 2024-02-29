use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(untagged)]
pub enum EnvFile {
    Simple(String),
    List(Vec<String>),
}

