use serde_json::json;
use uuid::Uuid;

use super::mock;
use super::utils::is_plan_higher_tier;
use super::{CategoryInfo, ProductInfo, UserProfile, UserServiceConnector};

/// Test that get_user_profile returns user with products list
#[tokio::test]
async fn test_mock_get_user_profile_returns_user_with_products() {
    let connector = mock::MockUserServiceConnector;
    let profile = connector.get_user_profile("test_token").await.unwrap();

    // Assertions on user profile structure
    assert_eq!(profile.email, "test@example.com");
    assert!(profile.plan.is_some());

    // Verify products list is populated
    assert!(!profile.products.is_empty());

    // Check for plan product
    let plan_product = profile.products.iter().find(|p| p.product_type == "plan");
    assert!(plan_product.is_some());
    assert_eq!(plan_product.unwrap().code, "professional");

    // Check for template product
    let template_product = profile
        .products
        .iter()
        .find(|p| p.product_type == "template");
    assert!(template_product.is_some());
    assert_eq!(template_product.unwrap().name, "AI Agent Stack Pro");
    assert_eq!(template_product.unwrap().external_id, Some(100));
}

/// Test that get_template_product returns product info for owned templates
#[tokio::test]
async fn test_mock_get_template_product_returns_product_info() {
    let connector = mock::MockUserServiceConnector;

    // Test with template ID that exists (100)
    let product = connector.get_template_product(100).await.unwrap();
    assert!(product.is_some());

    let prod = product.unwrap();
    assert_eq!(prod.id, "uuid-product-ai");
    assert_eq!(prod.name, "AI Agent Stack Pro");
    assert_eq!(prod.code, "ai-agent-stack-pro");
    assert_eq!(prod.product_type, "template");
    assert_eq!(prod.external_id, Some(100));
    assert_eq!(prod.price, Some(99.99));
    assert_eq!(prod.currency, Some("USD".to_string()));
    assert!(prod.is_active);
}

/// Test that get_template_product returns None for non-existent templates
#[tokio::test]
async fn test_mock_get_template_product_not_found() {
    let connector = mock::MockUserServiceConnector;

    // Test with non-existent template ID
    let product = connector.get_template_product(999).await.unwrap();
    assert!(product.is_none());
}

/// Test that user_owns_template correctly identifies owned templates
#[tokio::test]
async fn test_mock_user_owns_template_owned() {
    let connector = mock::MockUserServiceConnector;

    // Test with owned template ID
    let owns = connector
        .user_owns_template("test_token", "100")
        .await
        .unwrap();
    assert!(owns);

    // Test with code containing "ai-agent"
    let owns_code = connector
        .user_owns_template("test_token", "ai-agent-stack-pro")
        .await
        .unwrap();
    assert!(owns_code);
}

/// Test that user_owns_template returns false for non-owned templates
#[tokio::test]
async fn test_mock_user_owns_template_not_owned() {
    let connector = mock::MockUserServiceConnector;

    // Test with non-owned template ID
    let owns = connector
        .user_owns_template("test_token", "999")
        .await
        .unwrap();
    assert!(!owns);

    // Test with random code that doesn't match
    let owns_code = connector
        .user_owns_template("test_token", "random-template")
        .await
        .unwrap();
    assert!(!owns_code);
}

/// Test that user_has_plan always returns true in mock (for testing)
#[tokio::test]
async fn test_mock_user_has_plan() {
    let connector = mock::MockUserServiceConnector;

    let has_professional = connector
        .user_has_plan("user_123", "professional")
        .await
        .unwrap();
    assert!(has_professional);

    let has_enterprise = connector
        .user_has_plan("user_123", "enterprise")
        .await
        .unwrap();
    assert!(has_enterprise);

    let has_basic = connector.user_has_plan("user_123", "basic").await.unwrap();
    assert!(has_basic);
}

/// Test that get_user_plan returns correct plan info
#[tokio::test]
async fn test_mock_get_user_plan() {
    let connector = mock::MockUserServiceConnector;

    let plan = connector.get_user_plan("user_123").await.unwrap();
    assert_eq!(plan.user_id, "user_123");
    assert_eq!(plan.plan_name, "professional");
    assert!(plan.plan_description.is_some());
    assert_eq!(plan.plan_description.unwrap(), "Professional Plan");
    assert!(plan.active);
}

/// Test that list_available_plans returns multiple plan definitions
#[tokio::test]
async fn test_mock_list_available_plans() {
    let connector = mock::MockUserServiceConnector;

    let plans = connector.list_available_plans().await.unwrap();
    assert!(!plans.is_empty());
    assert_eq!(plans.len(), 3);

    // Verify specific plans exist
    let plan_names: Vec<String> = plans.iter().map(|p| p.name.clone()).collect();
    assert!(plan_names.contains(&"basic".to_string()));
    assert!(plan_names.contains(&"professional".to_string()));
    assert!(plan_names.contains(&"enterprise".to_string()));
}

/// Test that get_categories returns category list
#[tokio::test]
async fn test_mock_get_categories() {
    let connector = mock::MockUserServiceConnector;

    let categories = connector.get_categories().await.unwrap();
    assert!(!categories.is_empty());
    assert_eq!(categories.len(), 3);

    // Verify specific categories exist
    let category_names: Vec<String> = categories.iter().map(|c| c.name.clone()).collect();
    assert!(category_names.contains(&"cms".to_string()));
    assert!(category_names.contains(&"ecommerce".to_string()));
    assert!(category_names.contains(&"ai".to_string()));

    // Verify category has expected fields
    let ai_category = categories.iter().find(|c| c.name == "ai").unwrap();
    assert_eq!(ai_category.title, "AI Agents");
    assert_eq!(ai_category.priority, Some(5));
}

/// Test that create_stack_from_template returns stack with marketplace info
#[tokio::test]
async fn test_mock_create_stack_from_template() {
    let connector = mock::MockUserServiceConnector;
    let template_id = Uuid::new_v4();

    let stack = connector
        .create_stack_from_template(
            &template_id,
            "user_123",
            "1.0.0",
            "My Stack",
            json!({"services": []}),
        )
        .await
        .unwrap();

    assert_eq!(stack.user_id, "user_123");
    assert_eq!(stack.name, "My Stack");
    assert_eq!(stack.marketplace_template_id, Some(template_id));
    assert!(stack.is_from_marketplace);
    assert_eq!(stack.template_version, Some("1.0.0".to_string()));
}

/// Test that get_stack returns stack details
#[tokio::test]
async fn test_mock_get_stack() {
    let connector = mock::MockUserServiceConnector;

    let stack = connector.get_stack(1, "user_123").await.unwrap();
    assert_eq!(stack.id, 1);
    assert_eq!(stack.user_id, "user_123");
    assert_eq!(stack.name, "Test Stack");
}

/// Test that list_stacks returns user's stacks
#[tokio::test]
async fn test_mock_list_stacks() {
    let connector = mock::MockUserServiceConnector;

    let stacks = connector.list_stacks("user_123").await.unwrap();
    assert!(!stacks.is_empty());
    assert_eq!(stacks[0].user_id, "user_123");
}

/// Test plan hierarchy comparison
#[test]
fn test_is_plan_higher_tier_hierarchy() {
    // Enterprise user can access professional tier
    assert!(is_plan_higher_tier("enterprise", "professional"));

    // Enterprise user can access basic tier
    assert!(is_plan_higher_tier("enterprise", "basic"));

    // Professional user can access basic tier
    assert!(is_plan_higher_tier("professional", "basic"));

    // Basic user cannot access professional
    assert!(!is_plan_higher_tier("basic", "professional"));

    // Basic user cannot access enterprise
    assert!(!is_plan_higher_tier("basic", "enterprise"));

    // Same plan should not be considered upgrade
    assert!(!is_plan_higher_tier("professional", "professional"));
}

/// Test UserProfile deserialization with all fields
#[test]
fn test_user_profile_deserialization() {
    let json = json!({
        "email": "alice@example.com",
        "plan": {
            "name": "professional",
            "date_end": "2026-12-31"
        },
        "products": [
            {
                "id": "prod-1",
                "name": "Professional Plan",
                "code": "professional",
                "product_type": "plan",
                "external_id": null,
                "owned_since": "2025-01-01T00:00:00Z"
            },
            {
                "id": "prod-2",
                "name": "AI Stack",
                "code": "ai-stack",
                "product_type": "template",
                "external_id": 42,
                "owned_since": "2025-01-15T00:00:00Z"
            }
        ]
    });

    let profile: UserProfile = serde_json::from_value(json).unwrap();
    assert_eq!(profile.email, "alice@example.com");
    assert_eq!(profile.products.len(), 2);
    assert_eq!(profile.products[0].code, "professional");
    assert_eq!(profile.products[1].external_id, Some(42));
}

/// Test ProductInfo with optional fields
#[test]
fn test_product_info_deserialization() {
    let json = json!({
        "id": "product-123",
        "name": "AI Stack Template",
        "code": "ai-stack-template",
        "product_type": "template",
        "external_id": 42,
        "price": 99.99,
        "billing_cycle": "one_time",
        "currency": "USD",
        "vendor_id": 123,
        "is_active": true
    });

    let product: ProductInfo = serde_json::from_value(json).unwrap();
    assert_eq!(product.id, "product-123");
    assert_eq!(product.price, Some(99.99));
    assert_eq!(product.external_id, Some(42));
    assert_eq!(product.currency, Some("USD".to_string()));
}

/// Test CategoryInfo deserialization
#[test]
fn test_category_info_deserialization() {
    let json = json!({
        "_id": 5,
        "name": "ai",
        "title": "AI Agents",
        "priority": 5
    });

    let category: CategoryInfo = serde_json::from_value(json).unwrap();
    assert_eq!(category.id, 5);
    assert_eq!(category.name, "ai");
    assert_eq!(category.title, "AI Agents");
    assert_eq!(category.priority, Some(5));
}
