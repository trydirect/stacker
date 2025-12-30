use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct StackTemplate {
    pub id: Uuid,
    pub creator_user_id: String,
    pub creator_name: Option<String>,
    pub name: String,
    pub slug: String,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub category_id: Option<i32>,
    pub product_id: Option<i32>,
    pub tags: serde_json::Value,
    pub tech_stack: serde_json::Value,
    pub status: String,
    pub is_configurable: Option<bool>,
    pub view_count: Option<i32>,
    pub deploy_count: Option<i32>,
    pub required_plan_name: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct StackTemplateVersion {
    pub id: Uuid,
    pub template_id: Uuid,
    pub version: String,
    pub stack_definition: serde_json::Value,
    pub definition_format: Option<String>,
    pub changelog: Option<String>,
    pub is_latest: Option<bool>,
    pub created_at: Option<DateTime<Utc>>,
}
