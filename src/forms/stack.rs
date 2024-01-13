use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;
use std::collections::HashMap;
use std::fmt;
use crate::helpers::{login, docker_image_exists};

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

async fn validate_dockerhub_image(
    dockerhub_user: &Option<String>,
    dockerhub_password: &Option<String>,
    dockerhub_name: &Option<String>) -> Result<(), serde_valid::validation::Error> {

    let result = login(
        dockerhub_user.clone().unwrap_or("".to_string()).as_ref(),
        dockerhub_password.clone().unwrap_or("".to_string()).as_ref()
    ).await;

    let exists: bool = match result.unwrap().token {
        Some(tok) => {
            let exists = docker_image_exists(
                dockerhub_user.clone().unwrap().as_str(),
                dockerhub_name.clone().unwrap().as_str(), tok).await;
            match exists {
                Ok(_) => true,
                Err(err) => {
                    println!("{:?}", err);
                    false
                }
            }
        }
        _ => false,
    };
    if !exists {
        return Err(serde_valid::validation::Error::Custom(
            "Could not access docker image repository, please check credentials.".to_owned(),
        ));
    }
    Ok(())
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[validate(custom(|s| validate_dockerhub_image(dockerhub_user,dockerhub_name, dockerhub_password)))]
pub struct DockerImage {
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    #[validate(pattern = r"^[a-z0-9]+([-_.][a-z0-9]+)*$")]
    pub dockerhub_user: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    #[validate(pattern = r"^[a-z0-9]+([-_.][a-z0-9]+)*$")]
    pub dockerhub_name: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 100)]
    #[validate(pattern = r"^(?:(?=[^:\/]{1,253})(?!-)[a-zA-Z0-9-]{1,63}(?<!-)(?:\.(?!-)[a-zA-Z0-9-]{1,63}(?<!-))*(?::[0-9]{1,5})?/)?((?![._-])(?:[a-z0-9._-]*)(?<![._-])(?:/(?![._-])[a-z0-9._-]*(?<![._-]))*)(?::(?![.-])[a-zA-Z0-9_.-]{1,128})?$
")]
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


impl AsRef<DockerImage> for App {
    fn as_ref(&self) -> &DockerImage {
        &self.docker_image
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct StackForm {
    // #[validate(min_length=2)]
    // #[validate(max_length=255)]
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
    pub custom: Custom,
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
    pub docker_compose: Option<Vec<u8>>
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
    pub web: Vec<Web>,
    pub feature: Option<Vec<Feature>>,
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
    pub id: u32,
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
    pub docker_image: DockerImage,
    #[serde(flatten)]
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
    pub restart: Option<String>,
    pub volumes: Option<Vec<Volume>>,
    #[serde(flatten)]
    pub environment: Environment,
    #[serde(flatten)]
    pub network: ServiceNetworks,
    #[serde(rename(deserialize = "sharedPorts"))]
    #[serde(rename(serialize = "shared_ports"))]
    #[serde(alias = "shared_ports")]
    pub ports: Option<Vec<Port>>,
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
    pub app: App,
    pub custom: Option<bool>,
    pub main: Option<bool>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    #[serde(flatten)]
    pub app: App,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Service {
    #[serde(flatten)]
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
