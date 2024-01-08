use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;
use std::collections::HashMap;
use std::fmt;
use crate::forms;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Role {
    pub role: Option<Vec<String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Requirements {
    #[validate(min_length = 1)]
    #[validate(max_length = 10)]
    #[validate(pattern = r"^\d+\.?[0-9]+$")]
    pub cpu: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 10)]
    #[validate(pattern = r"^\d+G$")]
    #[serde(rename = "disk_size")]
    pub disk_size: Option<String>,
    #[serde(rename = "ram_size")]
    #[validate(min_length = 1)]
    #[validate(max_length = 10)]
    #[validate(pattern = r"^\d+G$")]
    pub ram_size: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Port {
    pub host_port: Option<String>,
    pub container_port: Option<String>,
}

// #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
// pub struct Ports {
//     #[serde(rename(deserialize = "sharedPorts"))]
//     #[serde(rename(serialize = "shared_ports"))]
//     // #[serde(alias = "shared_ports")]
//     pub shared_ports: Option<Vec<Port>>,
//     pub ports: Option<Vec<String>>,
// }

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerImage {
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub dockerhub_user: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub dockerhub_name: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 100)]
    pub dockerhub_image: Option<String>,
}

impl fmt::Display for DockerImage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let tag = "latest";

        let dim = self.dockerhub_image.clone().unwrap_or("".to_string());
        write!(
            f,
            "{}/{}:{}",
            self.dockerhub_user
                .clone()
                .unwrap_or("trydirect".to_string())
                .clone(),
            self.dockerhub_name.clone().unwrap_or(dim),
            tag
        )
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainList {}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Var {}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub value: f64,
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Network {
    name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub(crate) environment: Option<Vec<HashMap<String, String>>>,
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Volume {
    pub(crate) host_path: Option<String>,
    pub(crate) container_path: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Volumes {
    volumes: Vec<Volume>,
}

// pub(crate) type Networks = Option<Vec<String>>;
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceNetworks {
    pub network: Option<Vec<String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComposeNetworks {
    pub networks: Option<Vec<String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Web {
    #[serde(flatten)]
    pub app: forms::App,
    pub custom: Option<bool>,
    pub main: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    // #[serde(rename(deserialize = "sharedPorts"))]
    // #[serde(rename(serialize = "shared_ports"))]
    // #[serde(alias = "shared_ports")]
    // pub shared_ports: Option<Vec<Port>>,
    #[serde(flatten)]
    pub app: forms::App,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Service {
    // #[serde(rename(deserialize = "sharedPorts"))]
    // #[serde(rename(serialize = "shared_ports"))]
    // #[serde(alias = "shared_ports")]
    // pub shared_ports: Option<Vec<Port>>,
    #[serde(flatten)]
    pub(crate) app: forms::App,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub light: IconLight,
    pub dark: IconDark,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IconLight {
    pub width: i64,
    pub height: i64,
    pub image: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IconDark {}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Version {
    #[serde(rename = "_etag")]
    pub etag: Option<String>,
    #[serde(rename = "_id")]
    pub id: u32,
    #[serde(rename = "_created")]
    pub created: Option<String>,
    #[serde(rename = "_updated")]
    pub updated: Option<String>,
    pub app_id: Option<u32>,
    pub name: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 20)]
    pub version: String,
    #[serde(rename = "update_status")]
    pub update_status: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 20)]
    pub tag: String,
}
