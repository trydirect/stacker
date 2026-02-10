use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct StackCategory {
    pub id: i32,
    pub name: String,
    pub title: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct StackTemplate {
    pub id: Uuid,
    pub creator_user_id: String,
    pub creator_name: Option<String>,
    pub name: String,
    pub slug: String,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub category_code: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct StackTemplateReview {
    pub id: Uuid,
    pub template_id: Uuid,
    pub reviewer_user_id: Option<String>,
    pub decision: String,
    pub review_reason: Option<String>,
    pub security_checklist: Option<serde_json::Value>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub reviewed_at: Option<DateTime<Utc>>,
}
