use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;
use std::fmt;
use regex::Regex;
use crate::helpers::dockerhub::DockerHub;

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

fn validate_non_empty(v: &Option<String>) -> Result<(), serde_valid::validation::Error> {
    if v.is_none() {
        return Ok(());
    }

    if let Some(value) = v {
        if value.is_empty() {
            return Ok(());
        }

        // #[validate(pattern = r"^\d{2,6}+$")]
        let re = Regex::new(r"^\d{2,6}+$").unwrap();

        if !re.is_match(value.as_str()) {
            return Err(serde_valid::validation::Error::Custom("Port is not valid.".to_owned()));
        }
    }

    Ok(())
}

#[derive(Validate)]
struct Data {
    val: i32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Port {
    #[validate(custom(|v| validate_non_empty(v)))]
    pub host_port: Option<String>,
    #[validate(pattern = r"^\d{2,6}+$")]
    pub container_port: String,
    #[validate(enumerate("tcp", "udp"))]
    pub protocol: Option<String>,
}


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerImage {
    // #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    // @todo conditional check, if not empty
    // #[validate(pattern = r"^[a-z0-9]+([-_.][a-z0-9]+)*$")]
    pub dockerhub_user: Option<String>,
    // #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    // @todo conditional check, if not empty
    // #[validate(pattern = r"^[a-z0-9]+([-_.][a-z0-9]+)*$")]
    pub dockerhub_name: Option<String>,
    // #[validate(min_length = 3)]
    #[validate(max_length = 100)]
    pub dockerhub_image: Option<String>,
    pub dockerhub_password: Option<String>,
}

impl fmt::Display for DockerImage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let dh_image = self.dockerhub_image.as_ref().map(String::as_str).unwrap_or("");
        let dh_nmspc = self.dockerhub_user.as_ref().map(String::as_str).unwrap_or("");
        let dh_name = self.dockerhub_name.as_ref().map(String::as_str).unwrap_or("");

        write!(
            f,
            "{}{}{}",
            if !dh_nmspc.is_empty() { format!("{}/", dh_nmspc) } else { String::new() },
            if !dh_name.is_empty() { dh_name } else { dh_image },
            if !dh_name.contains(":") && dh_image.is_empty() { ":latest".to_string() } else { String::new() },
        )
    }
}


impl DockerImage {
    #[tracing::instrument(name = "is_active")]
    pub async fn is_active(&self) -> Result<bool, String> {
        DockerHub::from(self).is_active().await
    }
}


impl AsRef<DockerImage> for App {
    fn as_ref(&self) -> &DockerImage {
        &self.docker_image
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct StackForm {
    #[validate(max_length = 255)]
    #[serde(rename = "commonDomain")]
    pub common_domain: Option<String>,
    pub domain_list: Option<DomainList>,
    #[validate(min_length = 2)]
    #[validate(max_length = 255)]
    pub stack_code: Option<String>,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub region: String,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub zone: Option<String>,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub server: String,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub os: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub ssl: String,
    pub vars: Option<Vec<Var>>,
    pub integrated_features: Option<Vec<Value>>,
    pub extended_features: Option<Vec<Value>>,
    pub subscriptions: Option<Vec<String>>,
    pub form_app: Option<Vec<String>>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub disk_type: Option<String>,
    pub save_token: bool,
    #[validate(min_length = 10)]
    #[validate(max_length = 255)]
    pub cloud_token: String,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub provider: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub selected_plan: String,
    #[validate]
    pub custom: Custom,
}

impl StackForm {
    pub async fn is_readable_docker_image(&self) -> Result<bool, String> {
        let mut is_active = true;
        for app in &self.custom.web {
            if !app.app.docker_image.is_active().await? {
                is_active = false;
                break;
            }
        }

        // temporarily disabled
        // if let Some(service) = &self.custom.service {
        //     for app in service {
        //         if !app.app.docker_image.is_active().await? {
        //             is_active = false;
        //             break;
        //         }
        //     }
        // }
        //
        // if let Some(features) = &self.custom.feature {
        //     for app in features {
        //         if !app.app.docker_image.is_active().await? {
        //             is_active = false;
        //             break;
        //         }
        //     }
        // }
        Ok(is_active)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct StackPayload {
    pub(crate) id: Option<i32>,
    pub(crate) user_token: Option<String>,
    pub(crate) user_email: Option<String>,
    #[serde(rename = "commonDomain")]
    pub common_domain: String,
    pub domain_list: Option<DomainList>,
    pub region: String,
    pub zone: Option<String>,
    pub server: String,
    pub os: String,
    pub ssl: String,
    pub vars: Option<Vec<Var>>,
    #[serde(rename = "integrated_features")]
    pub integrated_features: Option<Vec<Value>>,
    #[serde(rename = "extended_features")]
    pub extended_features: Option<Vec<Value>>,
    pub subscriptions: Option<Vec<String>>,
    pub form_app: Option<Vec<String>>,
    pub disk_type: Option<String>,
    #[serde(rename = "save_token")]
    pub save_token: bool,
    #[serde(rename = "cloud_token")]
    pub cloud_token: String,
    pub provider: String,
    pub stack_code: String,
    #[serde(rename = "selected_plan")]
    pub selected_plan: String,
    pub custom: Custom,
    pub docker_compose: Option<Vec<u8>>,
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
pub struct Custom {
    #[validate]
    pub web: Vec<Web>,
    #[validate]
    pub feature: Option<Vec<Feature>>,
    #[validate]
    pub service: Option<Vec<Service>>,
    #[validate(minimum = 0)]
    #[validate(maximum = 10)]
    pub servers_count: u32,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub custom_stack_code: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 255)]
    pub project_git_url: Option<String>,
    pub custom_stack_category: Option<Vec<String>>,
    pub custom_stack_short_description: Option<String>,
    pub custom_stack_description: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 255)]
    pub project_name: String,
    pub project_overview: Option<String>,
    pub project_description: Option<String>,
    #[serde(flatten)]
    pub networks: ComposeNetworks, // all networks
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Network {
    name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct App {
    #[serde(rename = "_etag")]
    #[validate(min_length = 3)]
    #[validate(max_length = 255)]
    pub etag: Option<String>,
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "_created")]
    pub created: Option<String>,
    #[serde(rename = "_updated")]
    pub updated: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub name: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub code: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    #[serde(rename = "type")]
    pub type_field: String,
    #[serde(flatten)]
    pub role: Role,
    pub default: Option<bool>,
    pub versions: Option<Vec<Version>>,
    #[serde(flatten)]
    #[validate]
    pub docker_image: DockerImage,
    #[serde(flatten)]
    #[validate]
    pub requirements: Requirements,
    #[validate(minimum = 1)]
    pub popularity: Option<u32>,
    pub commercial: Option<bool>,
    pub subscription: Option<Value>,
    pub autodeploy: Option<bool>,
    pub suggested: Option<bool>,
    pub dependency: Option<Value>,
    pub avoid_render: Option<bool>,
    pub price: Option<Price>,
    pub icon: Option<Icon>,
    pub domain: Option<String>,
    pub category_id: Option<u32>,
    pub parent_app_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descr: Option<String>,
    pub full_description: Option<String>,
    pub description: Option<String>,
    pub plan_type: Option<String>,
    pub ansible_var: Option<String>,
    pub repo_dir: Option<String>,
    pub url_app: Option<String>,
    pub url_git: Option<String>,
    #[validate(enumerate("always", "no", "unless-stopped", "on-failure"))]
    pub restart: String,
    pub volumes: Option<Vec<Volume>>,
    #[serde(flatten)]
    pub environment: Environment,
    #[serde(flatten)]
    pub network: ServiceNetworks,
    #[validate]
    pub shared_ports: Option<Vec<Port>>,
}


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvVar {
    pub(crate) key: String,
    pub(crate) value: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub(crate) environment: Option<Vec<EnvVar>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Volume {
    pub host_path: Option<String>,
    pub container_path: Option<String>,
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

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Web {
    #[serde(flatten)]
    #[validate]
    pub app: App,
    pub custom: Option<bool>,
    pub main: Option<bool>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Feature {
    #[serde(flatten)]
    #[validate]
    pub app: App,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Service {
    #[serde(flatten)]
    #[validate]
    pub(crate) app: App,
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
