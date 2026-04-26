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
    pub price: Option<f64>,
    pub billing_cycle: Option<String>,
    pub currency: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub approved_at: Option<DateTime<Utc>>,
    pub verifications: serde_json::Value,
    pub infrastructure_requirements: serde_json::Value,
    pub public_ports: Option<serde_json::Value>,
    pub vendor_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InfrastructureRequirements {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_clouds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_os: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_ram_mb: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_disk_gb: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_cpu_cores: Option<i32>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct MarketplaceVendorProfile {
    pub creator_user_id: String,
    pub verification_status: String,
    pub onboarding_status: String,
    pub payouts_enabled: bool,
    pub payout_provider: Option<String>,
    pub payout_account_ref: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl MarketplaceVendorProfile {
    pub fn default_for_creator(creator_user_id: &str) -> Self {
        Self {
            creator_user_id: creator_user_id.to_string(),
            verification_status: "unverified".to_string(),
            onboarding_status: "not_started".to_string(),
            payouts_enabled: false,
            payout_provider: None,
            payout_account_ref: None,
            metadata: serde_json::json!({}),
            created_at: None,
            updated_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{InfrastructureRequirements, MarketplaceVendorProfile};

    #[test]
    fn infrastructure_requirements_default_is_empty() {
        let requirements = InfrastructureRequirements::default();

        assert!(requirements.supported_clouds.is_empty());
        assert!(requirements.supported_os.is_empty());
        assert_eq!(None, requirements.min_ram_mb);
        assert_eq!(None, requirements.min_disk_gb);
        assert_eq!(None, requirements.min_cpu_cores);
    }

    #[test]
    fn infrastructure_requirements_round_trip_serialization() {
        let requirements = InfrastructureRequirements {
            supported_clouds: vec!["hetzner".to_string(), "aws".to_string()],
            supported_os: vec!["ubuntu-22.04".to_string()],
            min_ram_mb: Some(2048),
            min_disk_gb: Some(20),
            min_cpu_cores: Some(2),
        };

        let value = serde_json::to_value(&requirements).expect("serialize requirements");
        let round_trip: InfrastructureRequirements =
            serde_json::from_value(value).expect("deserialize requirements");

        assert_eq!(requirements, round_trip);
    }

    #[test]
    fn infrastructure_requirements_partial_json_deserializes() {
        let requirements: InfrastructureRequirements =
            serde_json::from_value(serde_json::json!({ "min_ram_mb": 512 }))
                .expect("deserialize partial requirements");

        assert!(requirements.supported_clouds.is_empty());
        assert!(requirements.supported_os.is_empty());
        assert_eq!(Some(512), requirements.min_ram_mb);
        assert_eq!(None, requirements.min_disk_gb);
        assert_eq!(None, requirements.min_cpu_cores);
    }

    #[test]
    fn marketplace_vendor_profile_default_for_creator_is_safe() {
        let profile = MarketplaceVendorProfile::default_for_creator("creator-1");

        assert_eq!("creator-1", profile.creator_user_id);
        assert_eq!("unverified", profile.verification_status);
        assert_eq!("not_started", profile.onboarding_status);
        assert!(!profile.payouts_enabled);
        assert_eq!(serde_json::json!({}), profile.metadata);
    }
}
