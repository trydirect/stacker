use serde_json::{json, Value};

/// Load the shared marketplace-vendor-analytics contract.
///
/// Note: This contract is mirrored from ../shared-fixtures/api-contracts/marketplace-vendor-analytics.json
/// to stacker/tests/contracts/marketplace-vendor-analytics.contract.json for reliable CI access.
fn load_analytics_contract() -> Value {
    let contract_json = include_str!("contracts/marketplace-vendor-analytics.contract.json");
    serde_json::from_str(contract_json).expect("contract JSON should be valid")
}

#[test]
fn analytics_contract_has_correct_metadata() {
    let contract = load_analytics_contract();

    // Assert contract title
    assert_eq!(
        contract["title"].as_str().unwrap(),
        "marketplace-vendor-analytics",
        "Contract title must be marketplace-vendor-analytics"
    );

    // Assert owner is stacker
    assert_eq!(
        contract["_owner"].as_str().unwrap(),
        "stacker",
        "Contract owner must be stacker"
    );

    // Assert consumers include stacker and blog
    let consumers = contract["_consumers"]
        .as_array()
        .expect("_consumers should be an array");
    let consumer_strings: Vec<&str> = consumers.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        consumer_strings.contains(&"stacker"),
        "Consumers must include stacker"
    );
    assert!(
        consumer_strings.contains(&"blog"),
        "Consumers must include blog"
    );
}

#[test]
fn analytics_contract_defines_correct_endpoint() {
    let contract = load_analytics_contract();

    let endpoint = &contract["endpoints"]["vendor-template-analytics"];

    assert_eq!(
        endpoint["method"].as_str().unwrap(),
        "GET",
        "Endpoint method must be GET"
    );

    assert_eq!(
        endpoint["path"].as_str().unwrap(),
        "/api/templates/mine/analytics",
        "Endpoint path must be /api/templates/mine/analytics"
    );

    assert_eq!(
        endpoint["ownerService"].as_str().unwrap(),
        "stacker",
        "Endpoint ownerService must be stacker"
    );
}

#[test]
fn analytics_contract_period_enum_is_complete() {
    let contract = load_analytics_contract();

    let period_enum = contract["endpoints"]["vendor-template-analytics"]["query"]["period"]["enum"]
        .as_array()
        .expect("period enum should be an array");

    let period_values: Vec<&str> = period_enum.iter().filter_map(|v| v.as_str()).collect();

    assert!(period_values.contains(&"7d"), "period enum must include 7d");
    assert!(
        period_values.contains(&"30d"),
        "period enum must include 30d"
    );
    assert!(
        period_values.contains(&"90d"),
        "period enum must include 90d"
    );
    assert!(
        period_values.contains(&"all"),
        "period enum must include all"
    );
    assert!(
        period_values.contains(&"custom"),
        "period enum must include custom"
    );
}

#[test]
fn analytics_contract_bucket_enum_is_complete() {
    let contract = load_analytics_contract();

    let bucket_enum = contract["endpoints"]["vendor-template-analytics"]["response"]["properties"]
        ["period"]["properties"]["bucket"]["enum"]
        .as_array()
        .expect("bucket enum should be an array");

    let bucket_values: Vec<&str> = bucket_enum.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        bucket_values.contains(&"day"),
        "bucket enum must include day"
    );
    assert!(
        bucket_values.contains(&"week"),
        "bucket enum must include week"
    );
    assert!(
        bucket_values.contains(&"month"),
        "bucket enum must include month"
    );
    assert!(
        bucket_values.contains(&"all"),
        "bucket enum must include all"
    );
}

#[test]
fn analytics_response_matches_contract_shape() {
    let contract = load_analytics_contract();

    // Build a representative JSON analytics response following the contract structure
    let analytics_response = json!({
        "creatorId": "user-123",
        "period": {
            "key": "30d",
            "startDate": "2024-03-27T00:00:00Z",
            "endDate": "2024-04-26T23:59:59Z",
            "bucket": "day"
        },
        "summary": {
            "totalViews": 1250,
            "totalDeployments": 87,
            "conversionRate": 6.96,
            "publishedTemplates": 5,
            "topCloud": "hetzner",
            "topTemplateId": "550e8400-e29b-41d4-a716-446655440000"
        },
        "viewsSeries": [
            {
                "bucketStart": "2024-04-25T00:00:00Z",
                "bucketEnd": "2024-04-25T23:59:59Z",
                "count": 42
            },
            {
                "bucketStart": "2024-04-26T00:00:00Z",
                "bucketEnd": "2024-04-26T23:59:59Z",
                "count": 38
            }
        ],
        "deploymentsSeries": [
            {
                "bucketStart": "2024-04-25T00:00:00Z",
                "bucketEnd": "2024-04-25T23:59:59Z",
                "count": 3
            },
            {
                "bucketStart": "2024-04-26T00:00:00Z",
                "bucketEnd": "2024-04-26T23:59:59Z",
                "count": 2
            }
        ],
        "cloudBreakdown": [
            {
                "cloudProvider": "hetzner",
                "deployments": 45,
                "percentage": 51.72
            },
            {
                "cloudProvider": "digitalocean",
                "deployments": 28,
                "percentage": 32.18
            },
            {
                "cloudProvider": "aws",
                "deployments": 14,
                "percentage": 16.09
            }
        ],
        "topTemplates": [
            {
                "templateId": "550e8400-e29b-41d4-a716-446655440000",
                "productId": 101,
                "slug": "openclaw-aiworkbench",
                "name": "OpenClaw AI Workbench",
                "views": 520,
                "deployments": 45,
                "conversionRate": 8.65,
                "price": 29.99,
                "currency": "USD",
                "billingCycle": "monthly"
            },
            {
                "templateId": "660e8400-e29b-41d4-a716-446655440001",
                "productId": null,
                "slug": "django-postgres",
                "name": "Django PostgreSQL Stack",
                "views": 380,
                "deployments": 22,
                "conversionRate": 5.79,
                "price": null,
                "currency": null,
                "billingCycle": null
            }
        ],
        "templates": [
            {
                "templateId": "550e8400-e29b-41d4-a716-446655440000",
                "creatorUserId": "user-123",
                "productId": 101,
                "slug": "openclaw-aiworkbench",
                "name": "OpenClaw AI Workbench",
                "status": "published",
                "categoryCode": "ai-ml",
                "views": 520,
                "deployments": 45,
                "conversionRate": 8.65,
                "approvedAt": "2024-03-01T10:00:00Z",
                "updatedAt": "2024-04-15T14:30:00Z"
            },
            {
                "templateId": "660e8400-e29b-41d4-a716-446655440001",
                "creatorUserId": "user-123",
                "productId": null,
                "slug": "django-postgres",
                "name": "Django PostgreSQL Stack",
                "status": "published",
                "categoryCode": "web",
                "views": 380,
                "deployments": 22,
                "conversionRate": 5.79,
                "approvedAt": "2024-02-15T08:00:00Z",
                "updatedAt": "2024-04-10T16:20:00Z"
            }
        ]
    });

    // Get required top-level keys from contract
    let required_keys = contract["endpoints"]["vendor-template-analytics"]["response"]["required"]
        .as_array()
        .expect("required should be an array");

    let required_key_strings: Vec<&str> = required_keys.iter().filter_map(|v| v.as_str()).collect();

    // Assert all required top-level keys are present
    for key in &required_key_strings {
        assert!(
            analytics_response.get(key).is_some(),
            "Response must contain required key: {}",
            key
        );
    }

    // Verify required keys include expected fields
    assert!(required_key_strings.contains(&"creatorId"));
    assert!(required_key_strings.contains(&"period"));
    assert!(required_key_strings.contains(&"summary"));
    assert!(required_key_strings.contains(&"viewsSeries"));
    assert!(required_key_strings.contains(&"deploymentsSeries"));
    assert!(required_key_strings.contains(&"cloudBreakdown"));
    assert!(required_key_strings.contains(&"topTemplates"));
    assert!(required_key_strings.contains(&"templates"));
}

#[test]
fn analytics_response_has_required_period_fields() {
    let analytics_response = json!({
        "creatorId": "user-123",
        "period": {
            "key": "30d",
            "startDate": "2024-03-27T00:00:00Z",
            "endDate": "2024-04-26T23:59:59Z",
            "bucket": "day"
        },
        "summary": {
            "totalViews": 0,
            "totalDeployments": 0,
            "conversionRate": 0.0,
            "publishedTemplates": 0,
            "topCloud": null,
            "topTemplateId": null
        },
        "viewsSeries": [],
        "deploymentsSeries": [],
        "cloudBreakdown": [],
        "topTemplates": [],
        "templates": []
    });

    let period = &analytics_response["period"];

    // Assert required period fields
    assert!(period.get("key").is_some(), "period must have key");
    assert!(
        period.get("startDate").is_some(),
        "period must have startDate"
    );
    assert!(period.get("endDate").is_some(), "period must have endDate");
    assert!(period.get("bucket").is_some(), "period must have bucket");

    // Validate period.key is valid enum value
    let period_key = period["key"].as_str().unwrap();
    assert!(
        ["7d", "30d", "90d", "all", "custom"].contains(&period_key),
        "period.key must be valid enum value"
    );

    // Validate period.bucket is valid enum value
    let bucket = period["bucket"].as_str().unwrap();
    assert!(
        ["day", "week", "month", "all"].contains(&bucket),
        "period.bucket must be valid enum value"
    );
}

#[test]
fn analytics_response_has_required_summary_fields() {
    let analytics_response = json!({
        "creatorId": "user-123",
        "period": {
            "key": "30d",
            "startDate": null,
            "endDate": null,
            "bucket": "day"
        },
        "summary": {
            "totalViews": 100,
            "totalDeployments": 10,
            "conversionRate": 10.0,
            "publishedTemplates": 3,
            "topCloud": "hetzner",
            "topTemplateId": "550e8400-e29b-41d4-a716-446655440000"
        },
        "viewsSeries": [],
        "deploymentsSeries": [],
        "cloudBreakdown": [],
        "topTemplates": [],
        "templates": []
    });

    let summary = &analytics_response["summary"];

    // Assert required summary fields
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
    assert!(
        summary.get("topCloud").is_some(),
        "summary must have topCloud"
    );
    assert!(
        summary.get("topTemplateId").is_some(),
        "summary must have topTemplateId"
    );
}

#[test]
fn analytics_response_series_have_required_fields() {
    let series_item = json!({
        "bucketStart": "2024-04-26T00:00:00Z",
        "bucketEnd": "2024-04-26T23:59:59Z",
        "count": 42
    });

    // Assert required series item fields
    assert!(
        series_item.get("bucketStart").is_some(),
        "series item must have bucketStart"
    );
    assert!(
        series_item.get("bucketEnd").is_some(),
        "series item must have bucketEnd"
    );
    assert!(
        series_item.get("count").is_some(),
        "series item must have count"
    );
}

#[test]
fn analytics_response_cloud_breakdown_has_required_fields() {
    let cloud_item = json!({
        "cloudProvider": "hetzner",
        "deployments": 45,
        "percentage": 51.72
    });

    // Assert required cloud breakdown fields
    assert!(
        cloud_item.get("cloudProvider").is_some(),
        "cloud breakdown must have cloudProvider"
    );
    assert!(
        cloud_item.get("deployments").is_some(),
        "cloud breakdown must have deployments"
    );
    assert!(
        cloud_item.get("percentage").is_some(),
        "cloud breakdown must have percentage"
    );
}

#[test]
fn analytics_response_top_templates_have_required_fields() {
    let top_template = json!({
        "templateId": "550e8400-e29b-41d4-a716-446655440000",
        "slug": "openclaw-aiworkbench",
        "name": "OpenClaw AI Workbench",
        "views": 520,
        "deployments": 45,
        "conversionRate": 8.65
    });

    // Assert required top template fields
    assert!(
        top_template.get("templateId").is_some(),
        "top template must have templateId"
    );
    assert!(
        top_template.get("slug").is_some(),
        "top template must have slug"
    );
    assert!(
        top_template.get("name").is_some(),
        "top template must have name"
    );
    assert!(
        top_template.get("views").is_some(),
        "top template must have views"
    );
    assert!(
        top_template.get("deployments").is_some(),
        "top template must have deployments"
    );
    assert!(
        top_template.get("conversionRate").is_some(),
        "top template must have conversionRate"
    );
}

#[test]
fn analytics_response_templates_have_required_fields() {
    let template = json!({
        "templateId": "550e8400-e29b-41d4-a716-446655440000",
        "creatorUserId": "user-123",
        "slug": "openclaw-aiworkbench",
        "name": "OpenClaw AI Workbench",
        "status": "published",
        "views": 520,
        "deployments": 45
    });

    // Assert required template fields
    assert!(
        template.get("templateId").is_some(),
        "template must have templateId"
    );
    assert!(
        template.get("creatorUserId").is_some(),
        "template must have creatorUserId"
    );
    assert!(template.get("slug").is_some(), "template must have slug");
    assert!(template.get("name").is_some(), "template must have name");
    assert!(
        template.get("status").is_some(),
        "template must have status"
    );
    assert!(template.get("views").is_some(), "template must have views");
    assert!(
        template.get("deployments").is_some(),
        "template must have deployments"
    );
}

#[test]
fn analytics_response_excludes_finance_fields() {
    // Build a representative response
    let analytics_response = json!({
        "creatorId": "user-123",
        "period": {
            "key": "30d",
            "startDate": "2024-03-27T00:00:00Z",
            "endDate": "2024-04-26T23:59:59Z",
            "bucket": "day"
        },
        "summary": {
            "totalViews": 1250,
            "totalDeployments": 87,
            "conversionRate": 6.96,
            "publishedTemplates": 5,
            "topCloud": "hetzner",
            "topTemplateId": "550e8400-e29b-41d4-a716-446655440000"
        },
        "viewsSeries": [],
        "deploymentsSeries": [],
        "cloudBreakdown": [],
        "topTemplates": [],
        "templates": []
    });

    // Assert no finance fields at top level
    assert!(
        analytics_response.get("totalEarnings").is_none(),
        "Response must not contain totalEarnings"
    );
    assert!(
        analytics_response.get("totalAmount").is_none(),
        "Response must not contain totalAmount"
    );
    assert!(
        analytics_response.get("totalCreatorPayout").is_none(),
        "Response must not contain totalCreatorPayout"
    );
    assert!(
        analytics_response.get("pendingPayout").is_none(),
        "Response must not contain pendingPayout"
    );
    assert!(
        analytics_response.get("withdrawableBalance").is_none(),
        "Response must not contain withdrawableBalance"
    );
    assert!(
        analytics_response.get("pendingWithdrawalBalance").is_none(),
        "Response must not contain pendingWithdrawalBalance"
    );
    assert!(
        analytics_response.get("paidOutBalance").is_none(),
        "Response must not contain paidOutBalance"
    );
    assert!(
        analytics_response.get("payout").is_none(),
        "Response must not contain payout"
    );
    assert!(
        analytics_response.get("withdrawal").is_none(),
        "Response must not contain withdrawal"
    );
    assert!(
        analytics_response.get("balance").is_none(),
        "Response must not contain balance"
    );
    assert!(
        analytics_response.get("banking").is_none(),
        "Response must not contain banking"
    );

    // Assert no finance fields in summary
    let summary = &analytics_response["summary"];
    assert!(
        summary.get("totalEarnings").is_none(),
        "summary must not contain totalEarnings"
    );
    assert!(
        summary.get("revenue").is_none(),
        "summary must not contain revenue"
    );
    assert!(
        summary.get("earnings").is_none(),
        "summary must not contain earnings"
    );
}

#[test]
fn analytics_contract_notes_prohibit_finance_fields() {
    let contract = load_analytics_contract();

    let notes = contract["_notes"]
        .as_array()
        .expect("_notes should be an array");

    let notes_text: Vec<String> = notes
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
        .collect();

    // Check that at least one note mentions that Stacker analytics must not include finance fields
    let has_finance_prohibition = notes_text.iter().any(|note| {
        note.contains("withdrawal")
            || note.contains("payout")
            || note.contains("banking")
            || note.contains("balance")
    });

    assert!(
        has_finance_prohibition,
        "Contract notes must document that Stacker analytics excludes finance fields"
    );
}

#[test]
fn analytics_contract_describes_stacker_ownership() {
    let contract = load_analytics_contract();

    let description = contract["description"]
        .as_str()
        .expect("description should be a string")
        .to_lowercase();

    // Assert description mentions stacker's ownership of usage metrics
    assert!(
        description.contains("stacker"),
        "Description must mention Stacker"
    );
    assert!(
        description.contains("usage") || description.contains("metrics"),
        "Description must mention usage or metrics"
    );
}
