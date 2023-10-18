use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct StackForm {
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
    #[serde(rename = "save_token")]
    pub save_token: bool,
    #[serde(rename = "cloud_token")]
    pub cloud_token: String,
    pub provider: String,
    #[serde(rename = "stack_code")]
    pub stack_code: String,
    #[serde(rename = "selected_plan")]
    pub selected_plan: String,
    pub custom: Custom,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainList {
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Var {
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    pub value: f64
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct Custom {
    pub web: Vec<Web>,
    pub feature: Option<Vec<Feature>>,
    pub service: Option<Vec<Service>>,
    #[serde(rename = "servers_count")]
    pub servers_count: i64,
    #[serde(rename = "custom_stack_name")]
    pub custom_stack_name: String,
    #[serde(rename = "custom_stack_code")]
    pub custom_stack_code: String,
    #[serde(rename = "custom_stack_git_url")]
    pub custom_stack_git_url: Option<String>,
    #[serde(rename = "custom_stack_category")]
    pub custom_stack_category: Option<Vec<String>>,
    #[serde(rename = "custom_stack_short_description")]
    pub custom_stack_short_description: Option<String>,
    #[serde(rename = "custom_stack_description")]
    pub custom_stack_description: Option<String>,
    #[serde(rename = "project_name")]
    pub project_name: String,
    #[serde(rename = "project_overview")]
    pub project_overview: Option<String>,
    #[serde(rename = "project_description")]
    pub project_description: Option<String>,
}


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct Web {
    pub name: String,
    pub code: String,
    pub domain: Option<String>,
    pub shared_ports: Option<Vec<String>>,
    pub versions: Option<Vec<Version>>,
    pub custom: bool,
    #[serde(rename = "type")]
    pub type_field: String,
    pub main: bool,
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "dockerhub_user")]
    pub dockerhub_user: String,
    #[serde(rename = "dockerhub_name")]
    pub dockerhub_name: String,
    pub url_app: Option<String>,
    pub url_git: Option<String>,
    #[validate(min_length=1)]
    #[validate(max_length=10)]
    //#[validate(pattern = r"^\d+G$")]
    #[serde(rename = "disk_size")]
    pub disk_size: String,
    #[serde(rename = "ram_size")]
    #[validate(min_length=1)]
    #[validate(max_length=10)]
    //#[validate(pattern = r"^\d+G$")]
    pub ram_size: String,
    #[validate(minimum=0.1)]
    pub cpu: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    #[serde(rename = "_etag")]
    pub etag: Option<String>,
    #[serde(rename = "_id")]
    pub id: u32,
    #[serde(rename = "_created")]
    pub created: Option<String>,
    #[serde(rename = "_updated")]
    pub updated: Option<String>,
    pub name: String,
    pub code: String,
    pub role: Vec<String>,
    #[serde(rename = "type")]
    pub type_field: String,
    pub default: Option<bool>,
    pub popularity: Option<u32>,
    pub descr: Option<String>,
    pub ports: Option<Ports>,
    pub commercial: Option<bool>,
    pub subscription: Option<Value>,
    pub autodeploy: Option<bool>,
    pub suggested: Option<bool>,
    pub dependency: Option<Value>,
    #[serde(rename = "avoid_render")]
    pub avoid_render: Option<bool>,
    pub price: Option<Price>,
    pub icon: Option<Icon>,
    #[serde(rename = "category_id")]
    pub category_id: Option<u32>,
    #[serde(rename = "parent_app_id")]
    pub parent_app_id: Option<u32>,
    #[serde(rename = "full_description")]
    pub full_description: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "plan_type")]
    pub plan_type: Option<String>,
    #[serde(rename = "ansible_var")]
    pub ansible_var: Option<String>,
    #[serde(rename = "repo_dir")]
    pub repo_dir: Option<String>,
    #[validate(min_length=1)]
    pub cpu: String,
    #[validate(min_length=1)]
    #[serde(rename = "ram_size")]
    pub ram_size: String,
    #[validate(min_length=1)]
    #[serde(rename = "disk_size")]
    pub disk_size: String,
    #[serde(rename = "dockerhub_image")]
    pub dockerhub_image: Option<String>,
    pub versions: Option<Vec<Version>>,
    pub domain: Option<String>,
    pub shared_ports: Option<Vec<String>>,
    pub main: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ports {
    pub public: Vec<String>,
}


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Icon {
    pub light: IconLight,
    pub dark: IconDark,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconLight {
    pub width: i64,
    pub height: i64,
    pub image: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconDark {
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    #[serde(rename = "_etag")]
    pub etag: Option<String>,
    #[serde(rename = "_id")]
    pub id: i64,
    #[serde(rename = "_created")]
    pub created: Option<String>,
    #[serde(rename = "_updated")]
    pub updated: Option<String>,
    #[serde(rename = "app_id")]
    pub app_id: i64,
    pub name: String,
    pub version: String,
    #[serde(rename = "update_status")]
    pub update_status: Option<String>,
    pub tag: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    #[serde(rename = "_etag")]
    pub etag: Option<String>,
    #[serde(rename = "_id")]
    pub id: i64,
    #[serde(rename = "_created")]
    pub created: Option<String>,
    #[serde(rename = "_updated")]
    pub updated: Option<String>,
    pub name: String,
    pub code: String,
    pub role: Option<Vec<Value>>,
    #[serde(rename = "type")]
    pub type_field: String,
    pub default: Option<Value>,
    pub popularity: Option<u32>,
    pub descr: Option<String>,
    pub ports: Option<Ports>,
    pub commercial: Option<bool>,
    pub subscription: Option<Value>,
    pub autodeploy: Option<bool>,
    pub suggested: Option<bool>,
    pub dependency: Option<Value>,
    #[serde(rename = "avoid_render")]
    pub avoid_render: Option<bool>,
    pub price: Option<Price>,
    pub icon: Option<Icon>,
    #[serde(rename = "category_id")]
    pub category_id: Option<u32>,
    #[serde(rename = "parent_app_id")]
    pub parent_app_id: Option<u32>,
    #[serde(rename = "full_description")]
    pub full_description: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "plan_type")]
    pub plan_type: Option<String>,
    #[serde(rename = "ansible_var")]
    pub ansible_var: Option<String>,
    #[serde(rename = "repo_dir")]
    pub repo_dir: Option<String>,
    #[validate(min_length=1)]
    pub cpu: String,
    #[serde(rename = "ram_size")]
    #[validate(min_length=1)]
    pub ram_size: String,
    #[serde(rename = "disk_size")]
    #[validate(min_length=1)]
    pub disk_size: String,
    #[serde(rename = "dockerhub_image")]
    pub dockerhub_image: Option<String>,
    pub versions: Option<Vec<Version>>,
    pub domain: String,
    pub shared_ports: Option<Vec<String>>,
    pub main: bool,
}

