use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// #[derive(sqlx::Type, Debug, Clone, Copy)]
// #[sqlx(rename_all = "lowercase", type_name = "json")]
#[derive(Debug)]
pub struct Stack {
    pub id: i32,       // id - is a unique identifier for the app stack
    pub stack_id: Uuid, // external stack ID
    pub user_id: i32,  // external unique identifier for the user
    pub name: String,
    // pub body: sqlx::types::Json<String>,
    pub body: Value, //json type
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormData {
    pub common_domain: String,
    pub domain_list: DomainList,
    pub region: String,
    pub zone: Value,
    pub server: String,
    pub os: String,
    pub ssl: String,
    pub vars: Vec<Value>,
    #[serde(rename = "integrated_features")]
    pub integrated_features: Vec<Value>,
    #[serde(rename = "extended_features")]
    pub extended_features: Vec<Value>,
    pub subscriptions: Vec<String>,
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
pub struct DomainList {}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Custom {
    pub web: Vec<Web>,
    pub feature: Vec<Feature>,
    pub service: Vec<Service>,
    #[serde(rename = "servers_count")]
    pub servers_count: i64,
    #[serde(rename = "custom_stack_name")]
    pub custom_stack_name: String,
    #[serde(rename = "custom_stack_code")]
    pub custom_stack_code: String,
    #[serde(rename = "custom_stack_git_url")]
    pub custom_stack_git_url: String,
    #[serde(rename = "custom_stack_category")]
    pub custom_stack_category: Vec<String>,
    #[serde(rename = "custom_stack_short_description")]
    pub custom_stack_short_description: String,
    #[serde(rename = "custom_stack_description")]
    pub custom_stack_description: String,
    #[serde(rename = "project_name")]
    pub project_name: String,
    #[serde(rename = "project_overview")]
    pub project_overview: String,
    #[serde(rename = "project_description")]
    pub project_description: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Web {
    pub name: String,
    pub code: String,
    pub domain: String,
    pub shared_ports: Vec<String>,
    pub versions: Vec<Value>,
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
    #[serde(rename = "ram_size")]
    pub ram_size: String,
    pub cpu: i64,
    #[serde(rename = "disk_size")]
    pub disk_size: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    #[serde(rename = "_etag")]
    pub etag: Value,
    #[serde(rename = "_id")]
    pub id: i64,
    #[serde(rename = "_created")]
    pub created: String,
    #[serde(rename = "_updated")]
    pub updated: String,
    pub name: String,
    pub code: String,
    pub role: Vec<String>,
    #[serde(rename = "type")]
    pub type_field: String,
    pub default: Value,
    pub popularity: Value,
    pub descr: Value,
    pub ports: Ports,
    pub commercial: Value,
    pub subscription: Value,
    pub autodeploy: Value,
    pub suggested: Value,
    pub dependency: Value,
    #[serde(rename = "avoid_render")]
    pub avoid_render: Value,
    pub price: Value,
    pub icon: Icon,
    #[serde(rename = "category_id")]
    pub category_id: i64,
    #[serde(rename = "parent_app_id")]
    pub parent_app_id: Value,
    #[serde(rename = "full_description")]
    pub full_description: Value,
    pub description: String,
    #[serde(rename = "plan_type")]
    pub plan_type: Value,
    #[serde(rename = "ansible_var")]
    pub ansible_var: Value,
    #[serde(rename = "repo_dir")]
    pub repo_dir: Value,
    pub cpu: String,
    #[serde(rename = "ram_size")]
    pub ram_size: String,
    #[serde(rename = "disk_size")]
    pub disk_size: String,
    #[serde(rename = "dockerhub_image")]
    pub dockerhub_image: String,
    pub versions: Vec<Version>,
    pub domain: String,
    pub shared_ports: Vec<String>,
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
pub struct IconDark {}

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
    pub updated: String,
    #[serde(rename = "app_id")]
    pub app_id: i64,
    pub name: String,
    pub version: String,
    #[serde(rename = "update_status")]
    pub update_status: String,
    pub tag: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    #[serde(rename = "_etag")]
    pub etag: Value,
    #[serde(rename = "_id")]
    pub id: i64,
    #[serde(rename = "_created")]
    pub created: String,
    #[serde(rename = "_updated")]
    pub updated: String,
    pub name: String,
    pub code: String,
    pub role: Vec<Value>,
    #[serde(rename = "type")]
    pub type_field: String,
    pub default: Value,
    pub popularity: Value,
    pub descr: Value,
    pub ports: Value,
    pub commercial: Value,
    pub subscription: Value,
    pub autodeploy: Value,
    pub suggested: Value,
    pub dependency: Value,
    #[serde(rename = "avoid_render")]
    pub avoid_render: Value,
    pub price: Value,
    pub icon: Icon,
    #[serde(rename = "category_id")]
    pub category_id: Value,
    #[serde(rename = "parent_app_id")]
    pub parent_app_id: Value,
    #[serde(rename = "full_description")]
    pub full_description: Value,
    pub description: Value,
    #[serde(rename = "plan_type")]
    pub plan_type: Value,
    #[serde(rename = "ansible_var")]
    pub ansible_var: Value,
    #[serde(rename = "repo_dir")]
    pub repo_dir: Value,
    pub cpu: Value,
    #[serde(rename = "ram_size")]
    pub ram_size: Value,
    #[serde(rename = "disk_size")]
    pub disk_size: Value,
    #[serde(rename = "dockerhub_image")]
    pub dockerhub_image: String,
    pub versions: Vec<Version>,
    pub domain: String,
    pub shared_ports: Vec<String>,
    pub main: bool,
}