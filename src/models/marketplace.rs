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
    #[serde(default)]
    #[sqlx(default)]
    pub daily_rate: Option<f64>,
    #[serde(default)]
    #[sqlx(default)]
    pub monthly_cap: Option<f64>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub approved_at: Option<DateTime<Utc>>,
    pub verifications: serde_json::Value,
    pub infrastructure_requirements: serde_json::Value,
    pub public_ports: Option<serde_json::Value>,
    pub vendor_url: Option<String>,
    #[serde(default)]
    #[sqlx(default)]
    pub vendor_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub changelog: Option<String>,
    #[serde(default)]
    #[sqlx(default)]
    pub config_files: serde_json::Value,
    #[serde(default)]
    #[sqlx(default)]
    pub assets: serde_json::Value,
    #[serde(default)]
    #[sqlx(default)]
    pub seed_jobs: serde_json::Value,
    #[serde(default)]
    #[sqlx(default)]
    pub post_deploy_hooks: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub update_mode_capabilities: Option<serde_json::Value>,
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
    #[serde(default)]
    pub config_files: serde_json::Value,
    pub assets: serde_json::Value,
    #[serde(default)]
    pub seed_jobs: serde_json::Value,
    #[serde(default)]
    pub post_deploy_hooks: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_mode_capabilities: Option<serde_json::Value>,
    pub definition_format: Option<String>,
    pub changelog: Option<String>,
    pub is_latest: Option<bool>,
    /// Author-declared per-field policy (`config_contract`), stored in its own
    /// `stack_template_version.config_contract` column. `#[sqlx(default)]` so the
    /// many SELECTs that don't list the column still map (they get `None`); the
    /// federation reads (`get_latest_version`, `get_latest_version_by_template`,
    /// `get_by_slug_with_latest`) do list it so the webhook can deliver it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub config_contract: Option<serde_json::Value>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MarketplaceAsset {
    pub storage_provider: String,
    pub bucket: String,
    pub key: String,
    pub filename: String,
    pub sha256: String,
    pub size: i64,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetch_target: Option<String>,
    #[serde(default)]
    pub decompress: bool,
    #[serde(default)]
    pub executable: bool,
    #[serde(default = "default_true")]
    pub immutable: bool,
}

fn default_true() -> bool {
    true
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
    pub public_slug: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub website_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct PublicVendorProfile {
    pub creator_user_id: String,
    pub slug: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub website_url: Option<String>,
    pub verified: bool,
    pub rating: Option<f64>,
    pub rating_count: i64,
    pub rating_scale: i32,
    pub metadata: serde_json::Value,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct PublicVendorTemplate {
    pub id: Uuid,
    pub creator_user_id: String,
    pub creator_name: Option<String>,
    /// Public slug for the vendor's page, e.g. "acme-cloud".
    /// Use this — not `creator_user_id` — to build `/vendors/{slug}` URLs.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub vendor_slug: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub changelog: Option<String>,
    #[serde(default)]
    #[sqlx(default)]
    pub config_files: serde_json::Value,
    #[serde(default)]
    #[sqlx(default)]
    pub assets: serde_json::Value,
    #[serde(default)]
    #[sqlx(default)]
    pub seed_jobs: serde_json::Value,
    #[serde(default)]
    #[sqlx(default)]
    pub post_deploy_hooks: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub update_mode_capabilities: Option<serde_json::Value>,
    pub rating: Option<f64>,
    pub rating_count: i64,
    pub rating_scale: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TemplateRatingSummary {
    pub template_id: Uuid,
    pub rating: Option<f64>,
    pub rating_count: i64,
    pub rating_scale: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MyTemplateRating {
    pub template_id: Uuid,
    pub rating_id: i32,
    pub rating: f64,
    pub rating_scale: i32,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PublicVendorPage {
    pub vendor: PublicVendorProfile,
    pub templates: Vec<PublicVendorTemplate>,
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
            public_slug: None,
            display_name: None,
            bio: None,
            avatar_url: None,
            website_url: None,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Analytics Models (TDD: defined for test compilation)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Marketplace event for analytics tracking
/// Tracks view and deploy events with template_id, user, cloud_provider, timestamp, metadata
/// NOTE: Does NOT include finance fields (no amount, revenue, payout, withdrawal, balance)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct MarketplaceEvent {
    pub id: Uuid,
    pub template_id: Uuid,
    pub event_type: String, // "view" or "deploy"
    pub viewer_user_id: Option<String>,
    pub deployer_user_id: Option<String>,
    pub cloud_provider: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// Vendor analytics response model
/// Contains usage metrics only - NO finance fields
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorAnalytics {
    pub creator_id: String,
    pub period: AnalyticsPeriod,
    pub summary: AnalyticsSummary,
    pub views_series: Vec<SeriesBucket>,
    pub deployments_series: Vec<SeriesBucket>,
    pub cloud_breakdown: Vec<CloudBreakdown>,
    pub top_templates: Vec<TemplatePerformance>,
    pub templates: Vec<TemplateAnalytics>,
}

/// Analytics period definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsPeriod {
    pub key: String, // "7d", "30d", "90d", "all", "custom"
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub bucket: String, // "day", "week", "month", "all"
}

/// Analytics summary metrics
/// NOTE: Does NOT include finance fields (no totalEarnings, revenue, earnings, payout)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsSummary {
    pub total_views: i64,
    pub total_deployments: i64,
    pub conversion_rate: f64,
    pub published_templates: i32,
    pub top_cloud: Option<String>,
    pub top_template_id: Option<Uuid>,
}

/// Time series bucket for views/deployments
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesBucket {
    pub bucket_start: DateTime<Utc>,
    pub bucket_end: DateTime<Utc>,
    pub count: i64,
}

/// Cloud provider deployment breakdown
/// NOTE: Does NOT include finance fields (no revenue, earnings)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBreakdown {
    pub cloud_provider: String,
    pub deployments: i64,
    pub percentage: f64,
}

/// Template performance metrics for top templates
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePerformance {
    pub template_id: Uuid,
    pub slug: String,
    pub name: String,
    pub views: i64,
    pub deployments: i64,
    pub conversion_rate: f64,
}

/// Template analytics with full details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateAnalytics {
    pub template_id: Uuid,
    pub creator_user_id: String,
    pub slug: String,
    pub name: String,
    pub status: String,
    pub views: i64,
    pub deployments: i64,
    pub conversion_rate: f64,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Listing description rules (marketplace review requirements)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Minimum length of `short_description`, in characters (trimmed).
pub const SHORT_DESCRIPTION_MIN_CHARS: usize = 20;
/// Maximum length of `short_description`, in characters (trimmed).
pub const SHORT_DESCRIPTION_MAX_CHARS: usize = 200;
/// Minimum length of `long_description`, in characters (trimmed).
pub const LONG_DESCRIPTION_MIN_CHARS: usize = 100;
/// Maximum length of `long_description`, in characters (trimmed).
pub const LONG_DESCRIPTION_MAX_CHARS: usize = 5000;

/// A listing description that fails [`validate_listing_descriptions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptionRuleViolation {
    /// The offending field: `short_description` or `long_description`.
    pub field: &'static str,
    /// Human-readable explanation including the expected range.
    pub message: String,
}

/// All listing description violations for the given values — one per field at
/// most, so a caller can report every problem at once (`stacker config
/// validate`).
pub fn listing_description_violations(
    short_description: Option<&str>,
    long_description: Option<&str>,
) -> Vec<DescriptionRuleViolation> {
    let mut violations = Vec::new();
    if let Err(violation) = validate_description(
        "short_description",
        short_description,
        SHORT_DESCRIPTION_MIN_CHARS,
        SHORT_DESCRIPTION_MAX_CHARS,
    ) {
        violations.push(violation);
    }
    if let Err(violation) = validate_description(
        "long_description",
        long_description,
        LONG_DESCRIPTION_MIN_CHARS,
        LONG_DESCRIPTION_MAX_CHARS,
    ) {
        violations.push(violation);
    }
    violations
}

/// Enforce the listing description requirements from
/// `docs/MARKETPLACE_PUBLISH.md` ("Required metadata"): a short description of
/// 20-200 characters and a long description of 100-5000 characters. Marketplace
/// review rejects templates whose descriptions are missing or too short, so the
/// submit paths (`stacker submit` and `POST /api/templates/{id}/submit`) check
/// them before a submission reaches a reviewer. Reports the first violation;
/// use [`listing_description_violations`] to collect all of them.
pub fn validate_listing_descriptions(
    short_description: Option<&str>,
    long_description: Option<&str>,
) -> Result<(), DescriptionRuleViolation> {
    listing_description_violations(short_description, long_description)
        .into_iter()
        .next()
        .map_or(Ok(()), Err)
}

fn validate_description(
    field: &'static str,
    value: Option<&str>,
    min_chars: usize,
    max_chars: usize,
) -> Result<(), DescriptionRuleViolation> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(DescriptionRuleViolation {
            field,
            message: format!("{field} is missing ({min_chars}-{max_chars} characters required)"),
        });
    };
    let len = value.chars().count();
    if len < min_chars {
        return Err(DescriptionRuleViolation {
            field,
            message: format!("{field} is too short: {len} characters, minimum {min_chars}"),
        });
    }
    if len > max_chars {
        return Err(DescriptionRuleViolation {
            field,
            message: format!("{field} is too long: {len} characters, maximum {max_chars}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{InfrastructureRequirements, MarketplaceVendorProfile, StackTemplateVersion};

    // TDD (red): the federation fix (docs/MARKETPLACE_FIELD_POLICY.md §8) requires the
    // model to CARRY config_contract so the marketplace webhook builder can read
    // `latest_version.config_contract`. Today the column exists in the DB
    // (stack_template_version.config_contract, via db::marketplace::set/get_config_contract)
    // but is NOT mapped on this struct, so it is silently dropped on (de)serialize —
    // which is exactly why the policy never federates. Failing until the field is added.
    #[test]
    fn stack_template_version_carries_config_contract() {
        let raw = serde_json::json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "template_id": "22222222-2222-2222-2222-222222222222",
            "version": "1.0.0",
            "stack_definition": "services: {}\n",
            "assets": {},
            "config_contract": {
                "services": {
                    "web": {
                        "fields": {
                            "JWT_SECRET": { "mutability": "generated", "type": "hex", "length": 32 }
                        }
                    }
                }
            }
        });

        let version: StackTemplateVersion =
            serde_json::from_value(raw).expect("deserialize StackTemplateVersion");
        let round_trip = serde_json::to_value(&version).expect("serialize StackTemplateVersion");

        assert!(
            round_trip.get("config_contract").is_some(),
            "StackTemplateVersion must map the config_contract column so the webhook \
             payload can federate it; it is being dropped (federation gap §8)"
        );
        assert_eq!(
            round_trip["config_contract"]["services"]["web"]["fields"]["JWT_SECRET"]["mutability"],
            "generated"
        );
    }

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

    fn short_of(len: usize) -> String {
        "s".repeat(len)
    }

    fn long_of(len: usize) -> String {
        "l".repeat(len)
    }

    #[test]
    fn listing_descriptions_missing_are_violations() {
        let missing_short =
            super::validate_listing_descriptions(None, Some(&long_of(100))).unwrap_err();
        assert_eq!("short_description", missing_short.field);
        assert!(
            missing_short.message.contains("missing"),
            "{}",
            missing_short.message
        );

        let missing_long =
            super::validate_listing_descriptions(Some(&short_of(20)), None).unwrap_err();
        assert_eq!("long_description", missing_long.field);
        assert!(
            missing_long.message.contains("missing"),
            "{}",
            missing_long.message
        );

        let blank =
            super::validate_listing_descriptions(Some("   "), Some(&long_of(100))).unwrap_err();
        assert_eq!("short_description", blank.field);
        assert!(blank.message.contains("missing"), "{}", blank.message);
    }

    #[test]
    fn listing_descriptions_boundaries() {
        assert!(
            super::validate_listing_descriptions(Some(&short_of(20)), Some(&long_of(100))).is_ok()
        );
        assert!(
            super::validate_listing_descriptions(Some(&short_of(200)), Some(&long_of(5000)))
                .is_ok()
        );

        let too_short_short =
            super::validate_listing_descriptions(Some(&short_of(19)), Some(&long_of(100)))
                .unwrap_err();
        assert_eq!("short_description", too_short_short.field);
        assert!(
            too_short_short.message.contains("too short"),
            "{}",
            too_short_short.message
        );

        let too_long_short =
            super::validate_listing_descriptions(Some(&short_of(201)), Some(&long_of(100)))
                .unwrap_err();
        assert_eq!("short_description", too_long_short.field);
        assert!(
            too_long_short.message.contains("too long"),
            "{}",
            too_long_short.message
        );

        let too_short_long =
            super::validate_listing_descriptions(Some(&short_of(20)), Some(&long_of(99)))
                .unwrap_err();
        assert_eq!("long_description", too_short_long.field);
        assert!(
            too_short_long.message.contains("too short"),
            "{}",
            too_short_long.message
        );

        let too_long_long =
            super::validate_listing_descriptions(Some(&short_of(20)), Some(&long_of(5001)))
                .unwrap_err();
        assert_eq!("long_description", too_long_long.field);
        assert!(
            too_long_long.message.contains("too long"),
            "{}",
            too_long_long.message
        );
    }

    #[test]
    fn listing_descriptions_count_trimmed_characters() {
        let padded = format!("{}   ", short_of(20));
        let padded_long = format!("  {}", long_of(100));
        assert!(super::validate_listing_descriptions(Some(&padded), Some(&padded_long)).is_ok());
    }

    #[test]
    fn listing_description_violations_collects_every_field() {
        let violations = super::listing_description_violations(Some("x"), Some("y"));
        assert_eq!(2, violations.len());
        assert_eq!("short_description", violations[0].field);
        assert_eq!("long_description", violations[1].field);
    }
}
