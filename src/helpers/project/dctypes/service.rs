use serde::{Deserialize, Serialize};
#[cfg(feature = "indexmap")]
use indexmap::IndexMap;
use crate::helpers::project::dctypes;
use serde_json::Value;
use derive_builder::*;

#[derive(Builder, Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
#[builder(setter(into), default)]
pub struct Service {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub privileged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<dctypes::Healthcheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deploy: Option<dctypes::Deploy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "build")]
    pub build_: Option<dctypes::BuildStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<String>,
    #[serde(default, skip_serializing_if = "dctypes::Ports::is_empty")]
    pub ports: dctypes::Ports,
    #[serde(default, skip_serializing_if = "dctypes::Environment::is_empty")]
    pub environment: dctypes::Environment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<String>,
    #[serde(default, skip_serializing_if = "dctypes::Labels::is_empty")]
    pub labels: dctypes::Labels,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmpfs: Option<dctypes::Tmpfs>,
    #[serde(default, skip_serializing_if = "dctypes::Ulimits::is_empty")]
    pub ulimits: dctypes::Ulimits,
    #[serde(default, skip_serializing_if = "dctypes::Volumes::is_empty")]
    pub volumes: dctypes::Volumes,
    #[serde(default, skip_serializing_if = "dctypes::Networks::is_empty")]
    pub networks: dctypes::Networks,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cap_add: Vec<String>,
    #[serde(default, skip_serializing_if = "dctypes::DependsOnOptions::is_empty")]
    pub depends_on: dctypes::DependsOnOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<dctypes::Command>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<dctypes::Entrypoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file: Option<dctypes::EnvFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_grace_period: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_signal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expose: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes_from: Vec<String>,
    #[cfg(feature = "indexmap")]
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub extends: IndexMap<String, String>,
    #[cfg(not(feature = "indexmap"))]
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extends: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<dctypes::LoggingParameters>,
    #[serde(default, skip_serializing_if = "dctypes::is_zero")]
    pub scale: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub init: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stdin_open: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_size: Option<String>,
    #[cfg(feature = "indexmap")]
    #[serde(flatten, skip_serializing_if = "IndexMap::is_empty")]
    pub extensions: IndexMap<dctypes::Extension, Value>,
    #[cfg(not(feature = "indexmap"))]
    #[serde(flatten, skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<Extension, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_hosts: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tty: bool,
    #[serde(default, skip_serializing_if = "dctypes::SysCtls::is_empty")]
    pub sysctls: dctypes::SysCtls,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_opt: Vec<String>,
}

impl Service {
    pub fn image(&self) -> &str {
        self.image.as_deref().unwrap_or_default()
    }

    pub fn network_mode(&self) -> &str {
        self.network_mode.as_deref().unwrap_or_default()
    }
}

