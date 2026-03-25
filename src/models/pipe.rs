use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::types::uuid::Uuid;
use sqlx::types::JsonValue;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PipeTemplate — reusable pipe definitions
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PipeTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub source_app_type: String,
    pub source_endpoint: JsonValue,
    pub target_app_type: String,
    pub target_endpoint: JsonValue,
    pub target_external_url: Option<String>,
    pub field_mapping: JsonValue,
    pub config: Option<JsonValue>,
    pub is_public: Option<bool>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PipeTemplate {
    pub fn new(
        name: String,
        source_app_type: String,
        source_endpoint: JsonValue,
        target_app_type: String,
        target_endpoint: JsonValue,
        field_mapping: JsonValue,
        created_by: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            source_app_type,
            source_endpoint,
            target_app_type,
            target_endpoint,
            target_external_url: None,
            field_mapping,
            config: None,
            is_public: Some(false),
            created_by,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    pub fn with_external_url(mut self, url: String) -> Self {
        self.target_external_url = Some(url);
        self
    }

    pub fn with_config(mut self, config: JsonValue) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_public(mut self, is_public: bool) -> Self {
        self.is_public = Some(is_public);
        self
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PipeStatus — pipe instance lifecycle states
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipeStatus {
    Draft,
    Active,
    Paused,
    Error,
}

impl std::fmt::Display for PipeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipeStatus::Draft => write!(f, "draft"),
            PipeStatus::Active => write!(f, "active"),
            PipeStatus::Paused => write!(f, "paused"),
            PipeStatus::Error => write!(f, "error"),
        }
    }
}

impl Default for PipeStatus {
    fn default() -> Self {
        PipeStatus::Draft
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PipeInstance — deployment-specific pipe activations
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PipeInstance {
    pub id: Uuid,
    pub template_id: Option<Uuid>,
    pub deployment_hash: String,
    pub source_container: String,
    pub target_container: Option<String>,
    pub target_url: Option<String>,
    pub field_mapping_override: Option<JsonValue>,
    pub config_override: Option<JsonValue>,
    pub status: String,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub trigger_count: i64,
    pub error_count: i64,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PipeInstance {
    pub fn new(
        deployment_hash: String,
        source_container: String,
        created_by: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            template_id: None,
            deployment_hash,
            source_container,
            target_container: None,
            target_url: None,
            field_mapping_override: None,
            config_override: None,
            status: PipeStatus::Draft.to_string(),
            last_triggered_at: None,
            trigger_count: 0,
            error_count: 0,
            created_by,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn with_template(mut self, template_id: Uuid) -> Self {
        self.template_id = Some(template_id);
        self
    }

    pub fn with_target_container(mut self, container: String) -> Self {
        self.target_container = Some(container);
        self
    }

    pub fn with_target_url(mut self, url: String) -> Self {
        self.target_url = Some(url);
        self
    }

    pub fn with_field_mapping_override(mut self, mapping: JsonValue) -> Self {
        self.field_mapping_override = Some(mapping);
        self
    }

    pub fn with_config_override(mut self, config: JsonValue) -> Self {
        self.config_override = Some(config);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_pipe_status_display() {
        assert_eq!(PipeStatus::Draft.to_string(), "draft");
        assert_eq!(PipeStatus::Active.to_string(), "active");
        assert_eq!(PipeStatus::Paused.to_string(), "paused");
        assert_eq!(PipeStatus::Error.to_string(), "error");
    }

    #[test]
    fn test_pipe_status_default() {
        assert_eq!(PipeStatus::default(), PipeStatus::Draft);
    }

    #[test]
    fn test_pipe_status_serde_roundtrip() {
        let status = PipeStatus::Active;
        let serialized = serde_json::to_string(&status).unwrap();
        assert_eq!(serialized, "\"active\"");
        let deserialized: PipeStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, PipeStatus::Active);
    }

    #[test]
    fn test_pipe_template_new() {
        let template = PipeTemplate::new(
            "wordpress-to-mailchimp".to_string(),
            "wordpress".to_string(),
            json!({"path": "/wp-json/wp/v2/users", "method": "POST"}),
            "mailchimp".to_string(),
            json!({"path": "/3.0/lists/{list_id}/members", "method": "POST"}),
            json!({"email": "$.user_email", "name": "$.display_name"}),
            "user123".to_string(),
        );

        assert_eq!(template.name, "wordpress-to-mailchimp");
        assert_eq!(template.source_app_type, "wordpress");
        assert_eq!(template.target_app_type, "mailchimp");
        assert!(template.description.is_none());
        assert!(template.target_external_url.is_none());
        assert_eq!(template.is_public, Some(false));
        assert_eq!(template.created_by, "user123");
    }

    #[test]
    fn test_pipe_template_builder() {
        let template = PipeTemplate::new(
            "test-pipe".to_string(),
            "wordpress".to_string(),
            json!({}),
            "slack".to_string(),
            json!({}),
            json!({}),
            "user1".to_string(),
        )
        .with_description("A test pipe".to_string())
        .with_external_url("https://hooks.slack.com/services/xxx".to_string())
        .with_config(json!({"retry_count": 3}))
        .with_public(true);

        assert_eq!(template.description, Some("A test pipe".to_string()));
        assert_eq!(
            template.target_external_url,
            Some("https://hooks.slack.com/services/xxx".to_string())
        );
        assert_eq!(template.config, Some(json!({"retry_count": 3})));
        assert_eq!(template.is_public, Some(true));
    }

    #[test]
    fn test_pipe_template_serialization() {
        let template = PipeTemplate::new(
            "test".to_string(),
            "app_a".to_string(),
            json!({"path": "/api"}),
            "app_b".to_string(),
            json!({"path": "/hook"}),
            json!({"field1": "$.field2"}),
            "creator".to_string(),
        );

        let json_str = serde_json::to_string(&template).unwrap();
        let deserialized: PipeTemplate = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.source_app_type, "app_a");
        assert_eq!(deserialized.target_app_type, "app_b");
    }

    #[test]
    fn test_pipe_instance_new() {
        let instance = PipeInstance::new(
            "deploy_abc123".to_string(),
            "wordpress_1".to_string(),
            "user456".to_string(),
        );

        assert_eq!(instance.deployment_hash, "deploy_abc123");
        assert_eq!(instance.source_container, "wordpress_1");
        assert_eq!(instance.status, "draft");
        assert!(instance.template_id.is_none());
        assert!(instance.target_container.is_none());
        assert!(instance.target_url.is_none());
        assert_eq!(instance.trigger_count, 0);
        assert_eq!(instance.error_count, 0);
        assert!(instance.last_triggered_at.is_none());
    }

    #[test]
    fn test_pipe_instance_builder() {
        let template_id = Uuid::new_v4();
        let instance = PipeInstance::new(
            "deploy_xyz".to_string(),
            "wordpress_1".to_string(),
            "user789".to_string(),
        )
        .with_template(template_id)
        .with_target_container("mailchimp_1".to_string())
        .with_target_url("https://external.api/hook".to_string())
        .with_field_mapping_override(json!({"email": "$.custom_email"}))
        .with_config_override(json!({"timeout": 30}));

        assert_eq!(instance.template_id, Some(template_id));
        assert_eq!(instance.target_container, Some("mailchimp_1".to_string()));
        assert_eq!(
            instance.target_url,
            Some("https://external.api/hook".to_string())
        );
        assert_eq!(
            instance.field_mapping_override,
            Some(json!({"email": "$.custom_email"}))
        );
        assert_eq!(instance.config_override, Some(json!({"timeout": 30})));
    }

    #[test]
    fn test_pipe_instance_serialization() {
        let instance = PipeInstance::new(
            "deploy_test".to_string(),
            "container_a".to_string(),
            "user_test".to_string(),
        );

        let json_str = serde_json::to_string(&instance).unwrap();
        let deserialized: PipeInstance = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.deployment_hash, "deploy_test");
        assert_eq!(deserialized.source_container, "container_a");
        assert_eq!(deserialized.status, "draft");
    }
}
