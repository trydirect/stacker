/// TDD tests for marketplace metrics persistence and analytics queries
///
/// This test suite defines the contract for metrics storage and retrieval.
/// Tests are expected to FAIL initially until DB helpers and migrations are implemented.
///
/// Coverage:
/// 1. View/deploy event persistence with template_id, creator_user_id, event_type, cloud_provider, occurred_at, metadata
/// 2. Analytics queries are owner-scoped (creator A cannot see creator B's metrics)
/// 3. Period bucketing (7d/30d/custom) includes only events within period, zero-fills missing buckets
/// 4. Cloud breakdown groups deployments by provider and calculates percentages
/// 5. Aggregate fallback from stack_template.view_count/deploy_count when event table unavailable
/// 6. No finance fields in any metric structure
mod common;

use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 1: Event Persistence Contract
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Test that view events can be persisted with required fields
///
/// Expected to FAIL: needs marketplace_event table and insert_view_event helper
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_persist_view_event() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    let creator_user_id = "vendor-alice";
    let viewer_user_id = "user-bob";

    // Ensure template exists
    create_test_template(&app.db_pool, template_id, creator_user_id).await;

    // Record a view event
    let result = stacker::db::marketplace::insert_view_event(
        &app.db_pool,
        template_id,
        viewer_user_id,
        json!({"referrer": "https://trydirect.io/marketplace"}),
    )
    .await;

    assert!(
        result.is_ok(),
        "Should be able to insert view event: {:?}",
        result
    );

    // Verify event was persisted with all required fields
    // Using runtime query to avoid compile-time DB dependency
    let event_row = sqlx::query(
        r#"
        SELECT 
            template_id,
            event_type,
            viewer_user_id,
            occurred_at,
            metadata
        FROM marketplace_event
        WHERE template_id = $1 AND event_type = 'view'
        ORDER BY occurred_at DESC
        LIMIT 1
        "#,
    )
    .bind(template_id)
    .fetch_one(&app.db_pool)
    .await;

    // Expected to fail with "relation does not exist" until marketplace_event table is created
    match event_row {
        Ok(row) => {
            use sqlx::Row;
            let fetched_template_id: Uuid = row.get("template_id");
            let event_type: String = row.get("event_type");
            let viewer: Option<String> = row.get("viewer_user_id");

            assert_eq!(fetched_template_id, template_id);
            assert_eq!(event_type, "view");
            assert_eq!(viewer.as_deref(), Some(viewer_user_id));
        }
        Err(e) => {
            eprintln!("Expected failure: marketplace_event table does not exist yet: {}", e);
            panic!("TEST FAILED AS EXPECTED: marketplace_event table not implemented");
        }
    }
}

/// Test that deploy events can be persisted with cloud_provider field
///
/// Expected to FAIL: needs marketplace_event table and insert_deploy_event helper
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_persist_deploy_event_with_cloud_provider() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    let creator_user_id = "vendor-alice";
    let deployer_user_id = "user-charlie";
    let cloud_provider = "hetzner";

    create_test_template(&app.db_pool, template_id, creator_user_id).await;

    // Record a deploy event with cloud provider
    let result = stacker::db::marketplace::insert_deploy_event(
        &app.db_pool,
        template_id,
        deployer_user_id,
        cloud_provider,
        json!({"region": "eu-central", "server_ip": "192.168.1.100"}),
    )
    .await;

    assert!(
        result.is_ok(),
        "Should be able to insert deploy event: {:?}",
        result
    );

    // Verify event includes cloud_provider
    let event = sqlx::query(
        r#"
        SELECT 
            template_id,
            event_type,
            deployer_user_id,
            cloud_provider,
            occurred_at,
            metadata
        FROM marketplace_event
        WHERE template_id = $1 AND event_type = 'deploy'
        ORDER BY occurred_at DESC
        LIMIT 1
        "#,
    )
    .bind(template_id)
    .fetch_one(&app.db_pool)
    .await
    .expect("Should fetch inserted deploy event");

    let fetched_template_id: Uuid = event.get("template_id");
    let event_type: String = event.get("event_type");
    let deployer: Option<String> = event.get("deployer_user_id");
    let provider: Option<String> = event.get("cloud_provider");
    let occurred_at: Option<DateTime<Utc>> = event.get("occurred_at");
    let metadata: Option<serde_json::Value> = event.get("metadata");

    assert_eq!(fetched_template_id, template_id);
    assert_eq!(event_type, "deploy");
    assert_eq!(deployer.as_deref(), Some(deployer_user_id));
    assert_eq!(provider.as_deref(), Some(cloud_provider));
    assert!(occurred_at.is_some());
    assert!(metadata.is_some());
}

/// Test that occurred_at timestamp is automatically set
///
/// Expected to FAIL: needs marketplace_event table
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_event_occurred_at_timestamp_auto_set() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    let creator_user_id = "vendor-alice";

    create_test_template(&app.db_pool, template_id, creator_user_id).await;

    let before_insert = Utc::now();

    let result = stacker::db::marketplace::insert_view_event(
        &app.db_pool,
        template_id,
        "user-viewer",
        json!({}),
    )
    .await;

    assert!(result.is_ok(), "Should insert event");

    let after_insert = Utc::now();

    let event = sqlx::query(
        r#"
        SELECT occurred_at
        FROM marketplace_event
        WHERE template_id = $1
        ORDER BY occurred_at DESC
        LIMIT 1
        "#,
    )
    .bind(template_id)
    .fetch_one(&app.db_pool)
    .await
    .expect("Should fetch event");

    let occurred_at: Option<DateTime<Utc>> = event.get("occurred_at");
    let occurred_at = occurred_at.expect("occurred_at should be set");

    assert!(
        occurred_at >= before_insert && occurred_at <= after_insert,
        "occurred_at should be automatically set to current timestamp"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 2: Owner-scoped Analytics Queries
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Test that vendor analytics queries are scoped to creator_user_id
///
/// Expected to FAIL: needs get_vendor_analytics helper
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_analytics_scoped_to_creator() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let alice_template = Uuid::new_v4();
    let bob_template = Uuid::new_v4();

    // Create templates for two different vendors
    create_test_template(&app.db_pool, alice_template, "vendor-alice").await;
    create_test_template(&app.db_pool, bob_template, "vendor-bob").await;

    // Record events for Alice's template
    for _ in 0..5 {
        let _ = stacker::db::marketplace::insert_view_event(
            &app.db_pool,
            alice_template,
            "user-1",
            json!({}),
        )
        .await;
    }

    for _ in 0..3 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            alice_template,
            "user-1",
            "hetzner",
            json!({}),
        )
        .await;
    }

    // Record events for Bob's template
    for _ in 0..10 {
        let _ = stacker::db::marketplace::insert_view_event(
            &app.db_pool,
            bob_template,
            "user-2",
            json!({}),
        )
        .await;
    }

    for _ in 0..7 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            bob_template,
            "user-2",
            "digitalocean",
            json!({}),
        )
        .await;
    }

    // Query Alice's analytics - should only see her template data
    let alice_analytics = stacker::db::marketplace::get_vendor_analytics(
        &app.db_pool,
        "vendor-alice",
        None, // all time
    )
    .await
    .expect("Should fetch Alice's analytics");

    assert_eq!(
        alice_analytics.summary.total_views, 5,
        "Alice should see 5 views"
    );
    assert_eq!(
        alice_analytics.summary.total_deployments, 3,
        "Alice should see 3 deployments"
    );

    // Query Bob's analytics - should only see his template data
    let bob_analytics = stacker::db::marketplace::get_vendor_analytics(
        &app.db_pool,
        "vendor-bob",
        None, // all time
    )
    .await
    .expect("Should fetch Bob's analytics");

    assert_eq!(
        bob_analytics.summary.total_views, 10,
        "Bob should see 10 views"
    );
    assert_eq!(
        bob_analytics.summary.total_deployments, 7,
        "Bob should see 7 deployments"
    );

    // Alice's analytics should not include Bob's templates
    assert!(
        !alice_analytics
            .templates
            .iter()
            .any(|t| t.template_id == bob_template),
        "Alice's analytics should not include Bob's template"
    );

    // Bob's analytics should not include Alice's templates
    assert!(
        !bob_analytics
            .templates
            .iter()
            .any(|t| t.template_id == alice_template),
        "Bob's analytics should not include Alice's template"
    );
}

/// Test that cross-creator event queries are isolated
///
/// Expected to FAIL: needs query helpers with creator_user_id filtering
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_event_queries_isolated_by_creator() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let alice_template = Uuid::new_v4();
    let bob_template = Uuid::new_v4();

    create_test_template(&app.db_pool, alice_template, "vendor-alice").await;
    create_test_template(&app.db_pool, bob_template, "vendor-bob").await;

    // Create some events
    let _ = stacker::db::marketplace::insert_view_event(
        &app.db_pool,
        alice_template,
        "user-1",
        json!({}),
    )
    .await;
    let _ = stacker::db::marketplace::insert_view_event(
        &app.db_pool,
        bob_template,
        "user-2",
        json!({}),
    )
    .await;

    // Query events for Alice - should only return Alice's template events
    let alice_events = stacker::db::marketplace::get_template_events_by_creator(
        &app.db_pool,
        "vendor-alice",
        None,
        None,
    )
    .await
    .expect("Should fetch Alice's events");

    assert_eq!(alice_events.len(), 1, "Alice should have 1 event");
    assert_eq!(alice_events[0].template_id, alice_template);

    // Query events for Bob - should only return Bob's template events
    let bob_events = stacker::db::marketplace::get_template_events_by_creator(
        &app.db_pool,
        "vendor-bob",
        None,
        None,
    )
    .await
    .expect("Should fetch Bob's events");

    assert_eq!(bob_events.len(), 1, "Bob should have 1 event");
    assert_eq!(bob_events[0].template_id, bob_template);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 3: Period Bucketing and Zero-filling
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Test that 7-day period query only includes events within last 7 days
///
/// Expected to FAIL: needs period filtering in query helpers
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_period_7d_includes_only_recent_events() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    let now = Utc::now();
    let six_days_ago = now - Duration::days(6);
    let ten_days_ago = now - Duration::days(10);

    // Insert event within 7-day window
    insert_event_at_time(
        &app.db_pool,
        template_id,
        "view",
        six_days_ago,
        None,
        json!({}),
    )
    .await;

    // Insert event outside 7-day window
    insert_event_at_time(
        &app.db_pool,
        template_id,
        "view",
        ten_days_ago,
        None,
        json!({}),
    )
    .await;

    // Query for 7-day period
    let analytics = stacker::db::marketplace::get_vendor_analytics_for_period(
        &app.db_pool,
        "vendor-alice",
        "7d",
        None,
        None,
    )
    .await
    .expect("Should fetch analytics for 7d period");

    assert_eq!(
        analytics.summary.total_views, 1,
        "Should only count event within 7-day window"
    );
}

/// Test that 30-day period query filters correctly
///
/// Expected to FAIL: needs period filtering logic
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_period_30d_boundary_filtering() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    let now = Utc::now();
    let day_29 = now - Duration::days(29);
    let day_31 = now - Duration::days(31);

    // Event within 30-day window
    insert_event_at_time(&app.db_pool, template_id, "view", day_29, None, json!({})).await;

    // Event outside 30-day window
    insert_event_at_time(&app.db_pool, template_id, "view", day_31, None, json!({})).await;

    let analytics = stacker::db::marketplace::get_vendor_analytics_for_period(
        &app.db_pool,
        "vendor-alice",
        "30d",
        None,
        None,
    )
    .await
    .expect("Should fetch analytics for 30d period");

    assert_eq!(
        analytics.summary.total_views, 1,
        "Should only count event within 30-day window"
    );
}

/// Test that custom date range filters events correctly
///
/// Expected to FAIL: needs custom period support
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_custom_period_date_range_filtering() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    let start_date = Utc::now() - Duration::days(14);
    let end_date = Utc::now() - Duration::days(7);

    // Event before range
    insert_event_at_time(
        &app.db_pool,
        template_id,
        "view",
        start_date - Duration::days(1),
        None,
        json!({}),
    )
    .await;

    // Event within range
    insert_event_at_time(
        &app.db_pool,
        template_id,
        "view",
        start_date + Duration::days(3),
        None,
        json!({}),
    )
    .await;

    // Event after range
    insert_event_at_time(
        &app.db_pool,
        template_id,
        "view",
        end_date + Duration::days(1),
        None,
        json!({}),
    )
    .await;

    let analytics = stacker::db::marketplace::get_vendor_analytics_for_period(
        &app.db_pool,
        "vendor-alice",
        "custom",
        Some(start_date),
        Some(end_date),
    )
    .await
    .expect("Should fetch analytics for custom period");

    assert_eq!(
        analytics.summary.total_views, 1,
        "Should only count event within custom date range"
    );
}

/// Test that time series buckets are zero-filled when no data exists
///
/// Expected to FAIL: needs bucketing logic with zero-fill
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_time_series_zero_fill_missing_buckets() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    let now = Utc::now();

    // Insert event only on day 5 of the 7-day period
    insert_event_at_time(
        &app.db_pool,
        template_id,
        "view",
        now - Duration::days(2),
        None,
        json!({}),
    )
    .await;

    let analytics = stacker::db::marketplace::get_vendor_analytics_for_period(
        &app.db_pool,
        "vendor-alice",
        "7d",
        None,
        None,
    )
    .await
    .expect("Should fetch analytics");

    // Should return 7 buckets (one per day) even though only 1 has data
    assert_eq!(
        analytics.views_series.len(),
        7,
        "Should return 7 daily buckets for 7d period"
    );

    // Count how many buckets have zero views
    let zero_buckets = analytics.views_series.iter().filter(|b| b.count == 0).count();

    assert_eq!(
        zero_buckets, 6,
        "Should have 6 zero-filled buckets (only 1 day has data)"
    );

    // One bucket should have count = 1
    let non_zero_buckets = analytics
        .views_series
        .iter()
        .filter(|b| b.count > 0)
        .count();

    assert_eq!(non_zero_buckets, 1, "Should have 1 bucket with data");
}

/// Test that bucket granularity matches period (day/week/month)
///
/// Expected to FAIL: needs bucket calculation logic
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_bucket_granularity_matches_period() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    // Test 7d period returns daily buckets
    let analytics_7d = stacker::db::marketplace::get_vendor_analytics_for_period(
        &app.db_pool,
        "vendor-alice",
        "7d",
        None,
        None,
    )
    .await
    .expect("Should fetch 7d analytics");

    assert_eq!(
        analytics_7d.period.bucket, "day",
        "7d period should use daily buckets"
    );

    // Test 30d period returns daily buckets
    let analytics_30d = stacker::db::marketplace::get_vendor_analytics_for_period(
        &app.db_pool,
        "vendor-alice",
        "30d",
        None,
        None,
    )
    .await
    .expect("Should fetch 30d analytics");

    assert_eq!(
        analytics_30d.period.bucket, "day",
        "30d period should use daily buckets"
    );

    // Test 90d period might use weekly buckets (or daily, depending on implementation)
    let analytics_90d = stacker::db::marketplace::get_vendor_analytics_for_period(
        &app.db_pool,
        "vendor-alice",
        "90d",
        None,
        None,
    )
    .await
    .expect("Should fetch 90d analytics");

    assert!(
        ["day", "week"].contains(&analytics_90d.period.bucket.as_str()),
        "90d period should use day or week buckets"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 4: Cloud Breakdown with Percentages
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Test that cloud breakdown groups deployments by provider
///
/// Expected to FAIL: needs cloud breakdown query logic
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_cloud_breakdown_groups_by_provider() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    // Insert deploy events across different cloud providers
    for _ in 0..5 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-1",
            "hetzner",
            json!({}),
        )
        .await;
    }

    for _ in 0..3 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-2",
            "digitalocean",
            json!({}),
        )
        .await;
    }

    for _ in 0..2 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-3",
            "aws",
            json!({}),
        )
        .await;
    }

    let analytics = stacker::db::marketplace::get_vendor_analytics(
        &app.db_pool,
        "vendor-alice",
        None,
    )
    .await
    .expect("Should fetch analytics");

    // Should have 3 cloud providers in breakdown
    assert_eq!(
        analytics.cloud_breakdown.len(),
        3,
        "Should have breakdown for 3 cloud providers"
    );

    // Find hetzner entry
    let hetzner = analytics
        .cloud_breakdown
        .iter()
        .find(|c| c.cloud_provider == "hetzner")
        .expect("Should have hetzner in breakdown");

    assert_eq!(hetzner.deployments, 5);

    // Find digitalocean entry
    let digitalocean = analytics
        .cloud_breakdown
        .iter()
        .find(|c| c.cloud_provider == "digitalocean")
        .expect("Should have digitalocean in breakdown");

    assert_eq!(digitalocean.deployments, 3);

    // Find aws entry
    let aws = analytics
        .cloud_breakdown
        .iter()
        .find(|c| c.cloud_provider == "aws")
        .expect("Should have aws in breakdown");

    assert_eq!(aws.deployments, 2);
}

/// Test that cloud breakdown calculates correct percentages
///
/// Expected to FAIL: needs percentage calculation logic
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_cloud_breakdown_calculates_percentages() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    // Insert 100 total deploys: 50 hetzner, 30 digitalocean, 20 aws
    for _ in 0..50 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-1",
            "hetzner",
            json!({}),
        )
        .await;
    }

    for _ in 0..30 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-2",
            "digitalocean",
            json!({}),
        )
        .await;
    }

    for _ in 0..20 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-3",
            "aws",
            json!({}),
        )
        .await;
    }

    let analytics = stacker::db::marketplace::get_vendor_analytics(
        &app.db_pool,
        "vendor-alice",
        None,
    )
    .await
    .expect("Should fetch analytics");

    let hetzner = analytics
        .cloud_breakdown
        .iter()
        .find(|c| c.cloud_provider == "hetzner")
        .unwrap();

    // 50/100 = 50%
    assert!(
        (hetzner.percentage - 50.0).abs() < 0.1,
        "Hetzner should be ~50%, got {}",
        hetzner.percentage
    );

    let digitalocean = analytics
        .cloud_breakdown
        .iter()
        .find(|c| c.cloud_provider == "digitalocean")
        .unwrap();

    // 30/100 = 30%
    assert!(
        (digitalocean.percentage - 30.0).abs() < 0.1,
        "DigitalOcean should be ~30%, got {}",
        digitalocean.percentage
    );

    let aws = analytics
        .cloud_breakdown
        .iter()
        .find(|c| c.cloud_provider == "aws")
        .unwrap();

    // 20/100 = 20%
    assert!(
        (aws.percentage - 20.0).abs() < 0.1,
        "AWS should be ~20%, got {}",
        aws.percentage
    );

    // All percentages should sum to ~100%
    let total_percentage: f64 = analytics
        .cloud_breakdown
        .iter()
        .map(|c| c.percentage)
        .sum();

    assert!(
        (total_percentage - 100.0).abs() < 0.1,
        "Total percentage should be ~100%, got {}",
        total_percentage
    );
}

/// Test that cloud breakdown is sorted by deployment count descending
///
/// Expected to FAIL: needs ordering logic
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_cloud_breakdown_sorted_by_deployments_desc() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    // Insert in arbitrary order: aws(2), hetzner(10), digitalocean(5)
    for _ in 0..2 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-1",
            "aws",
            json!({}),
        )
        .await;
    }

    for _ in 0..10 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-2",
            "hetzner",
            json!({}),
        )
        .await;
    }

    for _ in 0..5 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-3",
            "digitalocean",
            json!({}),
        )
        .await;
    }

    let analytics = stacker::db::marketplace::get_vendor_analytics(
        &app.db_pool,
        "vendor-alice",
        None,
    )
    .await
    .expect("Should fetch analytics");

    // Should be sorted: hetzner(10), digitalocean(5), aws(2)
    assert_eq!(analytics.cloud_breakdown[0].cloud_provider, "hetzner");
    assert_eq!(analytics.cloud_breakdown[0].deployments, 10);

    assert_eq!(analytics.cloud_breakdown[1].cloud_provider, "digitalocean");
    assert_eq!(analytics.cloud_breakdown[1].deployments, 5);

    assert_eq!(analytics.cloud_breakdown[2].cloud_provider, "aws");
    assert_eq!(analytics.cloud_breakdown[2].deployments, 2);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 5: Aggregate Fallback from stack_template counters
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Test that analytics can fall back to view_count/deploy_count when event table is empty
///
/// Expected to FAIL: needs fallback logic in analytics query
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_aggregate_fallback_when_no_events() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    // Set view_count and deploy_count directly (legacy data)
    sqlx::query(
        r#"
        UPDATE stack_template 
        SET view_count = 42, deploy_count = 15
        WHERE id = $1
        "#,
    )
    .bind(template_id)
    .execute(&app.db_pool)
    .await
    .expect("Should update counters");

    // Query analytics with no events in event table
    let analytics = stacker::db::marketplace::get_vendor_analytics(
        &app.db_pool,
        "vendor-alice",
        None,
    )
    .await
    .expect("Should fetch analytics");

    // Should fall back to aggregate counters
    assert_eq!(
        analytics.summary.total_views, 42,
        "Should use view_count when no events exist"
    );
    assert_eq!(
        analytics.summary.total_deployments, 15,
        "Should use deploy_count when no events exist"
    );
}

/// Test that event data takes precedence over aggregate counters when available
///
/// Expected to FAIL: needs hybrid logic to prefer events over aggregates
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_events_take_precedence_over_aggregates() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    // Set legacy aggregate counters
    sqlx::query(
        r#"
        UPDATE stack_template 
        SET view_count = 100, deploy_count = 50
        WHERE id = $1
        "#,
    )
    .bind(template_id)
    .execute(&app.db_pool)
    .await
    .expect("Should update counters");

    // Insert a few events (newer data)
    for _ in 0..5 {
        let _ = stacker::db::marketplace::insert_view_event(
            &app.db_pool,
            template_id,
            "user-1",
            json!({}),
        )
        .await;
    }

    for _ in 0..3 {
        let _ = stacker::db::marketplace::insert_deploy_event(
            &app.db_pool,
            template_id,
            "user-1",
            "hetzner",
            json!({}),
        )
        .await;
    }

    let analytics = stacker::db::marketplace::get_vendor_analytics(
        &app.db_pool,
        "vendor-alice",
        None,
    )
    .await
    .expect("Should fetch analytics");

    // Should use event data (5 views, 3 deploys) instead of aggregates (100, 50)
    assert_eq!(
        analytics.summary.total_views, 5,
        "Should use event count when events exist"
    );
    assert_eq!(
        analytics.summary.total_deployments, 3,
        "Should use event count when events exist"
    );
}

/// Test that aggregate fallback is used for templates with no events in period
///
/// Expected to FAIL: needs period-aware fallback logic
#[tokio::test]
#[ignore = "pending marketplace metrics implementation"]
async fn test_aggregate_fallback_for_period_with_no_events() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };

    let template_id = Uuid::new_v4();
    create_test_template(&app.db_pool, template_id, "vendor-alice").await;

    // Set aggregate counters
    sqlx::query(
        r#"
        UPDATE stack_template 
        SET view_count = 25, deploy_count = 10
        WHERE id = $1
        "#,
    )
    .bind(template_id)
    .execute(&app.db_pool)
    .await
    .expect("Should update counters");

    // Insert old events (outside 7-day period)
    insert_event_at_time(
        &app.db_pool,
        template_id,
        "view",
        Utc::now() - Duration::days(30),
        None,
        json!({}),
    )
    .await;

    // Query for 7-day period (no events in period)
    let analytics = stacker::db::marketplace::get_vendor_analytics_for_period(
        &app.db_pool,
        "vendor-alice",
        "7d",
        None,
        None,
    )
    .await
    .expect("Should fetch analytics");

    // For "all time" summary, should still show aggregate totals even if no events in selected period
    // Or: should show 0 for period-specific metrics
    // Implementation decision: let's say period filters should return 0 when no events in period
    assert_eq!(
        analytics.summary.total_views, 0,
        "Should show 0 for period with no events (not aggregate)"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 6: No Finance Fields in Metric Structures
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Test that VendorAnalytics struct does not contain finance fields
///
/// Expected to PASS: validates contract at compile time
#[test]
fn test_vendor_analytics_struct_excludes_finance_fields() {
    // This test validates at compile time that VendorAnalytics struct
    // does not have finance-related fields

    let analytics = stacker::models::marketplace::VendorAnalytics {
        creator_id: "test".to_string(),
        period: stacker::models::marketplace::AnalyticsPeriod {
            key: "30d".to_string(),
            start_date: None,
            end_date: None,
            bucket: "day".to_string(),
        },
        summary: stacker::models::marketplace::AnalyticsSummary {
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

    // These compile-time assertions ensure finance fields don't exist
    // If any of these lines compile, it would mean the field exists (which would be a failure)

    // Uncommenting these should cause compile errors:
    // let _ = analytics.total_earnings;
    // let _ = analytics.withdrawable_balance;
    // let _ = analytics.pending_payout;
    // let _ = analytics.summary.revenue;
    // let _ = analytics.summary.earnings;

    assert_eq!(analytics.creator_id, "test");
}

/// Test that AnalyticsSummary struct does not contain finance fields
///
/// Expected to PASS: validates contract at compile time
#[test]
fn test_analytics_summary_excludes_finance_fields() {
    let summary = stacker::models::marketplace::AnalyticsSummary {
        total_views: 100,
        total_deployments: 50,
        conversion_rate: 50.0,
        published_templates: 5,
        top_cloud: Some("hetzner".to_string()),
        top_template_id: None,
    };

    // Compile-time validation that these fields don't exist:
    // let _ = summary.total_earnings;
    // let _ = summary.revenue;
    // let _ = summary.payout;

    assert!(summary.total_views > 0);
}

/// Test that CloudBreakdown struct does not contain finance fields
///
/// Expected to PASS: validates contract at compile time
#[test]
fn test_cloud_breakdown_excludes_finance_fields() {
    let breakdown = stacker::models::marketplace::CloudBreakdown {
        cloud_provider: "hetzner".to_string(),
        deployments: 50,
        percentage: 50.0,
    };

    // Compile-time validation:
    // let _ = breakdown.revenue;
    // let _ = breakdown.earnings;

    assert_eq!(breakdown.cloud_provider, "hetzner");
}

/// Test that marketplace event model does not include finance fields
///
/// Expected to FAIL: needs MarketplaceEvent model definition
#[test]
fn test_marketplace_event_excludes_finance_fields() {
    // This will fail until MarketplaceEvent model is defined
    let event = stacker::models::marketplace::MarketplaceEvent {
        id: Uuid::new_v4(),
        template_id: Uuid::new_v4(),
        event_type: "view".to_string(),
        viewer_user_id: Some("user-1".to_string()),
        deployer_user_id: None,
        cloud_provider: None,
        occurred_at: Utc::now(),
        metadata: json!({}),
    };

    // Compile-time validation that finance fields don't exist:
    // let _ = event.amount;
    // let _ = event.revenue;
    // let _ = event.payout_amount;

    assert_eq!(event.event_type, "view");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test Helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Helper to create a test template in the database
async fn create_test_template(pool: &PgPool, id: Uuid, creator_user_id: &str) {
    sqlx::query(
        r#"
        INSERT INTO stack_template (
            id,
            creator_user_id,
            creator_name,
            name,
            slug,
            short_description,
            status,
            tags,
            tech_stack,
            view_count,
            deploy_count,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'approved', '[]'::jsonb, '{}'::jsonb, 0, 0, NOW(), NOW())
        "#,
    )
    .bind(id)
    .bind(creator_user_id)
    .bind(format!("{} Creator", creator_user_id))
    .bind(format!("Test Template {}", id))
    .bind(format!("test-template-{}", id))
    .bind("A test template")
    .execute(pool)
    .await
    .expect("Should insert test template");
}

/// Helper to insert an event with a specific timestamp (for testing period filtering)
async fn insert_event_at_time(
    pool: &PgPool,
    template_id: Uuid,
    event_type: &str,
    occurred_at: DateTime<Utc>,
    cloud_provider: Option<&str>,
    metadata: serde_json::Value,
) {
    let viewer_user_id = if event_type == "view" {
        Some("test-viewer")
    } else {
        None
    };
    let deployer_user_id = if event_type == "deploy" {
        Some("test-deployer")
    } else {
        None
    };

    sqlx::query(
        r#"
        INSERT INTO marketplace_event (
            template_id,
            event_type,
            viewer_user_id,
            deployer_user_id,
            cloud_provider,
            occurred_at,
            metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(template_id)
    .bind(event_type)
    .bind(viewer_user_id)
    .bind(deployer_user_id)
    .bind(cloud_provider)
    .bind(occurred_at)
    .bind(metadata)
    .execute(pool)
    .await
    .expect("Should insert event at specific time");
}
