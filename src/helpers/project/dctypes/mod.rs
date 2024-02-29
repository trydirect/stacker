mod port;
mod published_port;
mod compose_file;
mod single_service;
mod service;
mod sys_ctls;
mod compose;
mod env_file;
mod depends_on_options;
mod depends_condition;
mod logging_parameters;
mod ports;
mod environment;
mod extension;
mod extension_parse_error;
mod services;
mod labels;
mod tmpfs;
mod ulimit;
mod ulimits;
mod networks;
mod build_step;
mod advanced_build_step;
mod build_args;
mod advanced_networks;
mod advanced_network_settings;
mod top_level_volumes;
mod compose_volume;
mod external_volume;
mod compose_network;
mod compose_networks;
mod compose_network_setting_details;
mod external_network_setting_bool;
mod network_settings;
mod ipam;
mod ipam_config;

pub use port::*;
pub use published_port::*;
pub use compose_file::*;
pub use single_service::*;
pub use service::*;
pub use sys_ctls::*;
pub use compose::*;
pub use env_file::*;
pub use depends_on_options::*;
pub use depends_condition::*;
pub use logging_parameters::*;
pub use ports::*;
pub use environment::*;
pub use extension::*;
pub use extension_parse_error::*;
pub use services::*;
pub use labels::*;
pub use tmpfs::*;
pub use ulimit::*;
pub use ulimits::*;
pub use networks::*;
pub use build_step::*;
pub use advanced_build_step::*;
pub use build_args::*;
pub use advanced_networks::*;
pub use advanced_network_settings::*;
pub use top_level_volumes::*;
pub use compose_volume::*;
pub use external_volume::*;
pub use compose_networks::*;
pub use compose_network::*;
pub use compose_network_setting_details::*;
pub use external_network_setting_bool::*;
pub use network_settings::*;
pub use ipam::*;
pub use ipam_config::*;

use crate::helpers::project::dctypes;

use derive_builder::*;
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
#[cfg(not(feature = "indexmap"))]
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt;


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Deploy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicas: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_config: Option<UpdateConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Resources>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_policy: Option<RestartPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
}

fn is_zero(val: &i64) -> bool {
    *val == 0
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(deny_unknown_fields)]
pub struct Healthcheck {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<HealthcheckTest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub retries: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_period: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(untagged)]
pub enum HealthcheckTest {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Default)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Default)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub constraints: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub preferences: Vec<Preferences>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub spread: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Default)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub limits: Option<Limits>,
    pub reservations: Option<Limits>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Default)]
#[serde(deny_unknown_fields)]
pub struct RestartPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelism: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_failure_ratio: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(untagged)]
pub enum Volumes {
    Simple(Vec<String>),
    Advanced(Vec<AdvancedVolumes>),
}

impl Default for Volumes {
    fn default() -> Self {
        Self::Simple(Vec::new())
    }
}

impl Volumes {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Simple(v) => v.is_empty(),
            Self::Advanced(v) => v.is_empty(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(deny_unknown_fields)]
pub struct AdvancedVolumes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub target: String,
    #[serde(rename = "type")]
    pub _type: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<Bind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<Volume>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmpfs: Option<TmpfsSettings>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Default)]
#[serde(deny_unknown_fields)]
pub struct Bind {
    pub propagation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Default)]
#[serde(deny_unknown_fields)]
pub struct Volume {
    pub nocopy: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Default)]
#[serde(deny_unknown_fields)]
pub struct TmpfsSettings {
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(untagged)]
pub enum Command {
    Simple(String),
    Args(Vec<String>),
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(untagged)]
pub enum Entrypoint {
    Simple(String),
    List(Vec<String>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
#[serde(untagged)]
pub enum SingleValue {
    String(String),
    Bool(bool),
    Unsigned(u64),
    Signed(i64),
    Float(f64),
}

impl fmt::Display for SingleValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::String(s) => f.write_str(s),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Unsigned(u) => write!(f, "{u}"),
            Self::Signed(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl}"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Hash)]
#[serde(untagged)]
pub enum MapOrEmpty<T> {
    Map(T),
    Empty,
}

impl<T> Default for MapOrEmpty<T> {
    fn default() -> Self {
        Self::Empty
    }
}

impl<T> From<MapOrEmpty<T>> for Option<T> {
    fn from(value: MapOrEmpty<T>) -> Self {
        match value {
            MapOrEmpty::Map(t) => Some(t),
            MapOrEmpty::Empty => None,
        }
    }
}

impl<T> Serialize for MapOrEmpty<T>
    where
        T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
    {
        match self {
            Self::Map(t) => t.serialize(serializer),
            Self::Empty => {
                use serde::ser::SerializeMap;
                serializer.serialize_map(None)?.end()
            }
        }
    }
}
