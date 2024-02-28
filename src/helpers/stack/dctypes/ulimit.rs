use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum Ulimit {
    Single(i64),
    SoftHard { soft: i64, hard: i64 },
}
