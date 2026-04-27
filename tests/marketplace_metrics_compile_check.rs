/// Compile-time check that analytics models are defined correctly and exclude finance fields
///
/// This is a minimal test to verify the model structure compiles before running DB tests

use stacker::models::marketplace::*;
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;

#[test]
fn test_marketplace_event_model_compiles() {
    let event = MarketplaceEvent {
        id: Uuid::new_v4(),
        template_id: Uuid::new_v4(),
        event_type: "view".to_string(),
        viewer_user_id: Some("user-1".to_string()),
        deployer_user_id: None,
        cloud_provider: None,
        occurred_at: Utc::now(),
        metadata: json!({}),
    };

    assert_eq!(event.event_type, "view");
    
    // These should NOT compile if added (finance fields):
    // let _ = event.amount;
    // let _ = event.revenue;
}

#[test]
fn test_vendor_analytics_model_compiles() {
    let analytics = VendorAnalytics {
        creator_id: "vendor-1".to_string(),
        period: AnalyticsPeriod {
            key: "30d".to_string(),
            start_date: None,
            end_date: None,
            bucket: "day".to_string(),
        },
        summary: AnalyticsSummary {
            total_views: 100,
            total_deployments: 50,
            conversion_rate: 50.0,
            published_templates: 5,
            top_cloud: Some("hetzner".to_string()),
            top_template_id: None,
        },
        views_series: vec![],
        deployments_series: vec![],
        cloud_breakdown: vec![],
        top_templates: vec![],
        templates: vec![],
    };

    assert_eq!(analytics.creator_id, "vendor-1");
    
    // These should NOT compile if added (finance fields):
    // let _ = analytics.total_earnings;
    // let _ = analytics.withdrawable_balance;
}

#[test]
fn test_cloud_breakdown_model_compiles() {
    let breakdown = CloudBreakdown {
        cloud_provider: "hetzner".to_string(),
        deployments: 50,
        percentage: 50.0,
    };

    assert_eq!(breakdown.cloud_provider, "hetzner");
    
    // These should NOT compile if added (finance fields):
    // let _ = breakdown.revenue;
    // let _ = breakdown.earnings;
}

#[test]
fn test_analytics_summary_model_compiles() {
    let summary = AnalyticsSummary {
        total_views: 100,
        total_deployments: 50,
        conversion_rate: 50.0,
        published_templates: 5,
        top_cloud: Some("hetzner".to_string()),
        top_template_id: None,
    };

    assert_eq!(summary.total_views, 100);
    
    // These should NOT compile if added (finance fields):
    // let _ = summary.total_earnings;
    // let _ = summary.revenue;
}
