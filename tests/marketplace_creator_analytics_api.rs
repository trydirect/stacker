/// TDD Integration tests for GET /api/templates/mine/analytics
///
/// This test suite validates the creator analytics endpoint behavior according to the
/// marketplace-vendor-analytics contract. These are HTTP-level tests using the full
/// Actix server stack (via `spawn_app`) with mock OAuth, real Postgres, and Casbin RBAC.
///
/// Expected Status: RED (tests compile but fail until production route implementation is complete)
///
/// Coverage:
/// 1. Authenticated creator receives analytics for their templates (200 with all required fields)
/// 2. Empty state: creator with no templates receives zeros in summary
/// 3. Period filtering: 7d/30d/90d/all/custom query params filter metrics correctly
/// 4. Cloud breakdown: groups deployments by provider with percentages
/// 5. Top templates: sorts by deployment or view count
/// 6. Access control: creator A cannot see creator B's analytics (403 or 404)
/// 7. Forbidden: unauthenticated request returns 403
/// 8. No finance fields: response excludes withdrawal, payout, earnings, balance
///
/// Why these tests exist:
/// - The analytics_contract.rs validates JSON shape; these validate HTTP behavior
/// - BDD scenarios in marketplace_creator.feature are high-level; these are granular
/// - Must prove authentication, authorization, period filtering, and owner-scoping work
/// - Catch regressions in route registration or RBAC policy changes
mod common;

use chrono::{Duration, Utc};
use reqwest::StatusCode;
use uuid::Uuid;

const BEARER_TOKEN_USER_A: &str = "test-bearer-token-user-a";
const MOCK_USER_A_ID: &str = "test_user_id"; // Default from common::mock_auth

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 1: Basic Success - Owner Receives Analytics
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Authenticated creator with templates receives 200 with complete analytics structure
///
/// Expected: RED until route implementation correctly aggregates metrics
#[tokio::test]
async fn analytics_returns_complete_structure_for_owner() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    // Seed a published template for the mock user
    let template_id = Uuid::new_v4();
    seed_template_with_events(&app.db_pool, template_id, MOCK_USER_A_ID, "approved").await;

    let response = client
        .get(format!("{}/api/templates/mine/analytics", app.address))
        .bearer_auth(BEARER_TOKEN_USER_A)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        StatusCode::OK,
        response.status(),
        "Owner should receive 200 for their analytics"
    );

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    // Assert all required top-level keys per contract
    assert!(
        body.get("creatorId").is_some(),
        "Response must contain creatorId"
    );
    assert!(body.get("period").is_some(), "Response must contain period");
    assert!(
        body.get("summary").is_some(),
        "Response must contain summary"
    );
    assert!(
        body.get("viewsSeries").is_some(),
        "Response must contain viewsSeries"
    );
    assert!(
        body.get("deploymentsSeries").is_some(),
        "Response must contain deploymentsSeries"
    );
    assert!(
        body.get("cloudBreakdown").is_some(),
        "Response must contain cloudBreakdown"
    );
    assert!(
        body.get("topTemplates").is_some(),
        "Response must contain topTemplates"
    );
    assert!(
        body.get("templates").is_some(),
        "Response must contain templates array"
    );

    // Verify period structure
    let period = body.get("period").unwrap();
    assert!(period.get("key").is_some(), "period must have key");
    assert!(
        period.get("startDate").is_some(),
        "period must have startDate"
    );
    assert!(period.get("endDate").is_some(), "period must have endDate");
    assert!(period.get("bucket").is_some(), "period must have bucket");

    // Verify summary structure
    let summary = body.get("summary").unwrap();
    assert!(
        summary.get("totalViews").is_some(),
        "summary must have totalViews"
    );
    assert!(
        summary.get("totalDeployments").is_some(),
        "summary must have totalDeployments"
    );
    assert!(
        summary.get("conversionRate").is_some(),
        "summary must have conversionRate"
    );
    assert!(
        summary.get("publishedTemplates").is_some(),
        "summary must have publishedTemplates"
    );

    println!("✓ Analytics response has complete contract-compliant structure");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 2: Empty State - No Templates
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Creator with no templates receives 200 with zero metrics
///
/// Expected: RED until route implementation handles empty state gracefully
#[tokio::test]
async fn analytics_returns_empty_state_for_new_creator() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    // No templates seeded - fresh creator
    let response = client
        .get(format!("{}/api/templates/mine/analytics", app.address))
        .bearer_auth(BEARER_TOKEN_USER_A)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        StatusCode::OK,
        response.status(),
        "New creator should receive 200 with empty analytics"
    );

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    let summary = body.get("summary").expect("Response must have summary");
    assert_eq!(
        0,
        summary["totalViews"].as_i64().unwrap_or(-1),
        "New creator should have 0 views"
    );
    assert_eq!(
        0,
        summary["totalDeployments"].as_i64().unwrap_or(-1),
        "New creator should have 0 deployments"
    );
    assert_eq!(
        0,
        summary["publishedTemplates"].as_i64().unwrap_or(-1),
        "New creator should have 0 published templates"
    );

    let templates = body
        .get("templates")
        .and_then(|t| t.as_array())
        .expect("templates should be an array");
    assert_eq!(0, templates.len(), "New creator should have empty templates array");

    println!("✓ Empty state returns zeros without error");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 3: Period Filtering - 7d vs 30d
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Period query param filters events correctly: 7d excludes older events
///
/// Expected: RED until route implementation respects period parameter
#[tokio::test]
async fn analytics_filters_by_period_7d() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    let template_id = Uuid::new_v4();
    seed_template_with_events(&app.db_pool, template_id, MOCK_USER_A_ID, "approved").await;

    // Insert events: recent (within 7d) and old (outside 7d)
    let recent_view_time = Utc::now() - Duration::days(3);
    let old_view_time = Utc::now() - Duration::days(20);

    insert_view_event(&app.db_pool, template_id, "user-recent", recent_view_time).await;
    insert_view_event(&app.db_pool, template_id, "user-old", old_view_time).await;

    // Request 7d period
    let response = client
        .get(format!(
            "{}/api/templates/mine/analytics?period=7d",
            app.address
        ))
        .bearer_auth(BEARER_TOKEN_USER_A)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::OK, response.status());

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    // Period key should reflect the requested filter
    assert_eq!(
        "7d",
        body["period"]["key"].as_str().unwrap_or_default(),
        "period.key should be 7d"
    );

    // Summary should count only recent event (1 view, not 2)
    let total_views = body["summary"]["totalViews"].as_i64().unwrap_or(-1);
    assert_eq!(
        1, total_views,
        "7d period should exclude events older than 7 days"
    );

    println!("✓ Period filtering works for 7d");
}

/// Period query param filters events correctly: 30d includes more events than 7d
///
/// Expected: RED until route implementation respects period parameter
#[tokio::test]
async fn analytics_filters_by_period_30d() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    let template_id = Uuid::new_v4();
    seed_template_with_events(&app.db_pool, template_id, MOCK_USER_A_ID, "approved").await;

    // Insert events: one within 7d, one within 30d but outside 7d
    let view_6d_ago = Utc::now() - Duration::days(6);
    let view_25d_ago = Utc::now() - Duration::days(25);

    insert_view_event(&app.db_pool, template_id, "user-recent", view_6d_ago).await;
    insert_view_event(&app.db_pool, template_id, "user-mid", view_25d_ago).await;

    // Request 30d period (default)
    let response = client
        .get(format!("{}/api/templates/mine/analytics", app.address))
        .bearer_auth(BEARER_TOKEN_USER_A)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::OK, response.status());

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    // Default period should be 30d
    assert_eq!(
        "30d",
        body["period"]["key"].as_str().unwrap_or_default(),
        "Default period should be 30d"
    );

    // Summary should count both events
    let total_views = body["summary"]["totalViews"].as_i64().unwrap_or(-1);
    assert_eq!(
        2, total_views,
        "30d period should include events from last 30 days"
    );

    println!("✓ Period filtering works for 30d (default)");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 4: Cloud Breakdown
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Cloud breakdown groups deployments by provider and calculates percentages
///
/// Expected: RED until route implementation aggregates by cloud_provider
#[tokio::test]
async fn analytics_includes_cloud_breakdown() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    let template_id = Uuid::new_v4();
    seed_template_with_events(&app.db_pool, template_id, MOCK_USER_A_ID, "approved").await;

    // Insert deploy events with different cloud providers
    let now = Utc::now();
    insert_deploy_event(&app.db_pool, template_id, "user-1", "hetzner", now).await;
    insert_deploy_event(&app.db_pool, template_id, "user-2", "hetzner", now).await;
    insert_deploy_event(&app.db_pool, template_id, "user-3", "digitalocean", now).await;

    let response = client
        .get(format!("{}/api/templates/mine/analytics", app.address))
        .bearer_auth(BEARER_TOKEN_USER_A)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::OK, response.status());

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    let cloud_breakdown = body
        .get("cloudBreakdown")
        .and_then(|cb| cb.as_array())
        .expect("cloudBreakdown should be an array");

    assert!(
        !cloud_breakdown.is_empty(),
        "cloudBreakdown should not be empty when deployments exist"
    );

    // Find hetzner entry
    let hetzner_entry = cloud_breakdown
        .iter()
        .find(|item| item["cloudProvider"].as_str() == Some("hetzner"));

    assert!(
        hetzner_entry.is_some(),
        "cloudBreakdown should include hetzner"
    );

    let hetzner = hetzner_entry.unwrap();
    assert_eq!(
        2,
        hetzner["deployments"].as_i64().unwrap_or(0),
        "Hetzner should have 2 deployments"
    );

    // Percentage should be approximately 66.67% (2 out of 3)
    let percentage = hetzner["percentage"].as_f64().unwrap_or(0.0);
    assert!(
        percentage > 60.0 && percentage < 70.0,
        "Hetzner percentage should be ~66.67%, got {}",
        percentage
    );

    println!("✓ Cloud breakdown aggregates by provider with percentages");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 5: Top Templates Sorting
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Top templates are sorted by deployment count
///
/// Expected: RED until route implementation sorts topTemplates correctly
#[tokio::test]
async fn analytics_sorts_top_templates_by_deployments() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    // Seed two templates with different deployment counts
    let template_popular = Uuid::new_v4();
    let template_less_popular = Uuid::new_v4();

    seed_template_with_events(
        &app.db_pool,
        template_popular,
        MOCK_USER_A_ID,
        "approved",
    )
    .await;
    seed_template_with_events(
        &app.db_pool,
        template_less_popular,
        MOCK_USER_A_ID,
        "approved",
    )
    .await;

    let now = Utc::now();
    // template_popular: 5 deployments
    for i in 0..5 {
        insert_deploy_event(
            &app.db_pool,
            template_popular,
            &format!("user-{}", i),
            "hetzner",
            now,
        )
        .await;
    }

    // template_less_popular: 2 deployments
    for i in 0..2 {
        insert_deploy_event(
            &app.db_pool,
            template_less_popular,
            &format!("user-{}", i + 10),
            "digitalocean",
            now,
        )
        .await;
    }

    let response = client
        .get(format!("{}/api/templates/mine/analytics", app.address))
        .bearer_auth(BEARER_TOKEN_USER_A)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::OK, response.status());

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    let top_templates = body
        .get("topTemplates")
        .and_then(|tt| tt.as_array())
        .expect("topTemplates should be an array");

    assert!(
        top_templates.len() >= 2,
        "topTemplates should include both seeded templates"
    );

    // First template should be the one with more deployments
    let first_deployments = top_templates[0]["deployments"].as_i64().unwrap_or(0);
    let second_deployments = top_templates[1]["deployments"].as_i64().unwrap_or(0);

    assert!(
        first_deployments >= second_deployments,
        "topTemplates should be sorted by deployments DESC"
    );

    println!("✓ Top templates sorted by deployment count");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 6: Access Control - Forbidden Access to Another Creator
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Creator A cannot see Creator B's analytics
///
/// Expected: RED until route implementation enforces owner-scoped queries
#[tokio::test]
async fn analytics_forbidden_for_another_creator() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    // Seed a template for a different user (creator B)
    let template_id = Uuid::new_v4();
    let creator_b_id = "other_creator_user_id";
    seed_template_with_events(&app.db_pool, template_id, creator_b_id, "approved").await;

    // User A attempts to query analytics (should only see their own - none exist)
    let response = client
        .get(format!("{}/api/templates/mine/analytics", app.address))
        .bearer_auth(BEARER_TOKEN_USER_A)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::OK, response.status());

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    let templates = body
        .get("templates")
        .and_then(|t| t.as_array())
        .expect("templates should be an array");

    // User A should NOT see creator B's template
    assert_eq!(
        0,
        templates.len(),
        "User A should not see other creator's templates"
    );

    let total_deployments = body["summary"]["totalDeployments"].as_i64().unwrap_or(-1);
    assert_eq!(
        0, total_deployments,
        "User A should not see other creator's deployments"
    );

    println!("✓ Analytics are correctly scoped to authenticated creator");
}

/// Creator cannot use templateId query param to access another creator's template analytics
///
/// Expected: RED until validate_optional_template_scope enforces ownership check
#[tokio::test]
async fn analytics_forbidden_for_template_not_owned() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    // Seed a template for a different user
    let template_id = Uuid::new_v4();
    let creator_b_id = "other_creator_user_id";
    seed_template_with_events(&app.db_pool, template_id, creator_b_id, "approved").await;

    // User A attempts to query analytics with templateId of creator B's template
    let response = client
        .get(format!(
            "{}/api/templates/mine/analytics?templateId={}",
            app.address, template_id
        ))
        .bearer_auth(BEARER_TOKEN_USER_A)
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
        "Should return 403 or 404 when templateId is not owned by creator, got {}",
        status
    );

    println!("✓ Template scope validation prevents access to other creator's templates");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 7: Unauthenticated Request
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Unauthenticated request returns 403 Forbidden
///
/// Expected: GREEN (authentication middleware should already enforce this)
#[tokio::test]
async fn analytics_returns_forbidden_without_authorization() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/templates/mine/analytics", app.address))
        // Intentionally omit .bearer_auth(…)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        StatusCode::FORBIDDEN,
        response.status(),
        "Missing auth should return 403 Forbidden"
    );

    println!("✓ Unauthenticated request correctly returns 403");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 8: No Finance Fields in Response
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Response must not include withdrawal, payout, earnings, balance fields
///
/// Expected: GREEN (contract test already validates this, but HTTP test confirms it end-to-end)
#[tokio::test]
async fn analytics_excludes_finance_fields() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => {
            eprintln!("Skipping test: database not available");
            return;
        }
    };
    let client = reqwest::Client::new();

    let template_id = Uuid::new_v4();
    seed_template_with_events(&app.db_pool, template_id, MOCK_USER_A_ID, "approved").await;

    let response = client
        .get(format!("{}/api/templates/mine/analytics", app.address))
        .bearer_auth(BEARER_TOKEN_USER_A)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::OK, response.status());

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    // Assert no finance fields at top level
    assert!(
        body.get("totalEarnings").is_none(),
        "Response must not contain totalEarnings"
    );
    assert!(
        body.get("totalAmount").is_none(),
        "Response must not contain totalAmount"
    );
    assert!(
        body.get("withdrawal").is_none(),
        "Response must not contain withdrawal"
    );
    assert!(
        body.get("payout").is_none(),
        "Response must not contain payout"
    );
    assert!(
        body.get("balance").is_none(),
        "Response must not contain balance"
    );
    assert!(
        body.get("banking").is_none(),
        "Response must not contain banking"
    );
    assert!(
        body.get("revenue").is_none(),
        "Response must not contain revenue"
    );

    // Assert no finance fields in summary
    let summary = body.get("summary").unwrap();
    assert!(
        summary.get("earnings").is_none(),
        "summary must not contain earnings"
    );
    assert!(
        summary.get("revenue").is_none(),
        "summary must not contain revenue"
    );

    println!("✓ Analytics response excludes all finance fields");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test Helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Seed a minimal template for testing
async fn seed_template_with_events(
    pool: &sqlx::PgPool,
    template_id: Uuid,
    creator_user_id: &str,
    status: &str,
) {
    let slug = format!("test-template-{}", template_id);
    sqlx::query(
        r#"
        INSERT INTO stack_template (
            id,
            creator_user_id,
            name,
            slug,
            status,
            tags,
            tech_stack,
            view_count,
            deploy_count
        )
        VALUES ($1, $2, $3, $4, $5, '[]', '{}', 0, 0)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(template_id)
    .bind(creator_user_id)
    .bind(format!("Test Template {}", template_id))
    .bind(slug)
    .bind(status)
    .execute(pool)
    .await
    .expect("Failed to seed template");
}

/// Insert a view event for testing period filtering
async fn insert_view_event(
    pool: &sqlx::PgPool,
    template_id: Uuid,
    viewer_user_id: &str,
    occurred_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        r#"
        INSERT INTO marketplace_template_event (
            template_id,
            event_type,
            viewer_user_id,
            metadata,
            occurred_at,
            created_at
        )
        VALUES ($1, 'view', $2, '{}', $3, NOW())
        "#,
    )
    .bind(template_id)
    .bind(viewer_user_id)
    .bind(occurred_at)
    .execute(pool)
    .await
    .expect("Failed to insert view event");
}

/// Insert a deploy event for testing cloud breakdown
async fn insert_deploy_event(
    pool: &sqlx::PgPool,
    template_id: Uuid,
    deployer_user_id: &str,
    cloud_provider: &str,
    occurred_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        r#"
        INSERT INTO marketplace_template_event (
            template_id,
            event_type,
            deployer_user_id,
            cloud_provider,
            metadata,
            occurred_at,
            created_at
        )
        VALUES ($1, 'deploy', $2, $3, '{}', $4, NOW())
        "#,
    )
    .bind(template_id)
    .bind(deployer_user_id)
    .bind(cloud_provider)
    .bind(occurred_at)
    .execute(pool)
    .await
    .expect("Failed to insert deploy event");
}
