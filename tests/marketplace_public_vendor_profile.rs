mod common;

use reqwest::StatusCode;
use serde_json::Value;

async fn seed_vendor_page(
    app: &common::TestApp,
    public_slug: &str,
) -> common::MarketplaceVendorFixture {
    let vendor = common::seed_marketplace_vendor_fixture(&app.db_pool, public_slug).await;
    common::seed_marketplace_template_fixtures_for_vendor(&app.db_pool, &vendor.creator_user_id)
        .await;
    common::seed_marketplace_template_ratings_for_vendor(&app.db_pool, &vendor.creator_user_id)
        .await;
    vendor
}

#[tokio::test]
async fn public_vendor_profile_returns_vendor_and_approved_templates_by_slug() {
    // Given a shared vendor fixture with approved and non-approved templates
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let vendor = seed_vendor_page(&app, "acme-cloud").await;

    // When a public user requests the vendor page without authentication
    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/vendors/{}",
            app.address, vendor.public_slug
        ))
        .send()
        .await
        .expect("Failed to fetch public vendor profile");

    // Then the public vendor profile and only approved templates are returned
    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("vendor profile response should be valid JSON");

    assert_eq!("OK", body["message"]);
    assert_eq!(
        vendor.creator_user_id,
        body["item"]["vendor"]["creator_user_id"]
    );
    assert_eq!(vendor.public_slug, body["item"]["vendor"]["slug"]);
    assert_eq!(vendor.display_name, body["item"]["vendor"]["display_name"]);
    assert_eq!(
        vendor.bio,
        body["item"]["vendor"]["bio"].as_str().map(str::to_string)
    );
    assert_eq!(true, body["item"]["vendor"]["verified"]);
    assert!(body["item"]["vendor"]["created_at"].is_string());
    assert_eq!(Some(4.0), body["item"]["vendor"]["rating"].as_f64());
    assert_eq!(3, body["item"]["vendor"]["rating_count"]);
    assert_eq!(5, body["item"]["vendor"]["rating_scale"]);

    let templates = body["item"]["templates"]
        .as_array()
        .expect("templates should be an array");
    assert_eq!(2, templates.len());

    let slugs = templates
        .iter()
        .map(|template| template["slug"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(slugs.contains(&"wordpress-pro"));
    assert!(slugs.contains(&"postgres-backup"));
    assert!(!slugs.contains(&"draft-internal-template"));

    let wordpress = templates
        .iter()
        .find(|template| template["slug"] == "wordpress-pro")
        .expect("wordpress template should be present");
    assert_eq!(Some(4.5), wordpress["rating"].as_f64());
    assert_eq!(2, wordpress["rating_count"]);
    assert_eq!(5, wordpress["rating_scale"]);

    let postgres = templates
        .iter()
        .find(|template| template["slug"] == "postgres-backup")
        .expect("postgres template should be present");
    assert_eq!(Some(3.0), postgres["rating"].as_f64());
    assert_eq!(1, postgres["rating_count"]);
    assert_eq!(5, postgres["rating_scale"]);
}

#[tokio::test]
async fn public_vendor_profile_can_be_loaded_by_creator_user_id() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let vendor = seed_vendor_page(&app, "acme-cloud").await;

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/vendors/{}",
            app.address, vendor.creator_user_id
        ))
        .send()
        .await
        .expect("Failed to fetch public vendor profile by creator id");

    assert_eq!(StatusCode::OK, response.status());
    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert_eq!(body["item"]["vendor"]["slug"], "acme-cloud");
}

#[tokio::test]
async fn public_vendor_profile_does_not_expose_sensitive_payout_fields() {
    // Given a vendor fixture with a payout account reference
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let vendor = seed_vendor_page(&app, "acme-cloud").await;
    sqlx::query(
        r#"UPDATE marketplace_vendor_profile
           SET metadata = metadata || '{"internal_note":"do-not-leak","onboarding":{"secret":"hidden"}}'::jsonb
           WHERE creator_user_id = $1"#,
    )
    .bind(&vendor.creator_user_id)
    .execute(&app.db_pool)
    .await
    .expect("Failed to add private metadata");

    // When the public vendor page is requested
    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/vendors/{}",
            app.address, vendor.public_slug
        ))
        .send()
        .await
        .expect("Failed to fetch public vendor profile");

    assert_eq!(StatusCode::OK, response.status());
    let body: Value = response
        .json()
        .await
        .expect("vendor profile response should be valid JSON");

    // Then payout account references are not exposed anywhere in the payload
    assert!(body["item"]["vendor"].get("payout_account_ref").is_none());
    assert!(!body.to_string().contains("acct_acme_fixture"));
    assert!(!body.to_string().contains("do-not-leak"));
    assert!(body["item"]["vendor"]["metadata"]
        .get("internal_note")
        .is_none());
    assert!(body["item"]["vendor"]["metadata"]
        .get("onboarding")
        .is_none());
}

#[tokio::test]
async fn public_vendor_profile_returns_not_found_for_unknown_vendor() {
    // Given no matching vendor fixture has been seeded
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    // When a public user requests an unknown vendor
    let response = reqwest::Client::new()
        .get(format!("{}/api/vendors/missing-vendor", app.address))
        .send()
        .await
        .expect("Failed to fetch missing public vendor profile");

    // Then the API returns not found
    assert_eq!(StatusCode::NOT_FOUND, response.status());
}

#[tokio::test]
async fn public_vendor_profile_does_not_require_authentication() {
    // Given a public vendor fixture exists
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let vendor = seed_vendor_page(&app, "acme-cloud").await;

    // When the request has no Authorization header
    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/vendors/{}",
            app.address, vendor.public_slug
        ))
        .send()
        .await
        .expect("Failed to fetch public vendor profile");

    // Then it still succeeds because vendor pages are public
    assert_eq!(StatusCode::OK, response.status());
}

#[tokio::test]
async fn public_vendor_profile_returns_empty_template_list_when_vendor_has_no_approved_templates() {
    // Given a vendor exists but only has draft templates
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let vendor = seed_vendor_page(&app, "beta-labs").await;

    // When the public vendor page is requested
    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/vendors/{}",
            app.address, vendor.public_slug
        ))
        .send()
        .await
        .expect("Failed to fetch public vendor profile");

    // Then the profile is returned with an empty templates array
    assert_eq!(StatusCode::OK, response.status());
    let body: Value = response
        .json()
        .await
        .expect("vendor profile response should be valid JSON");

    assert_eq!("beta-labs", body["item"]["vendor"]["slug"]);
    assert!(body["item"]["vendor"]["created_at"].is_string());
    assert!(body["item"]["vendor"]["rating"].is_null());
    assert_eq!(0, body["item"]["vendor"]["rating_count"]);
    assert_eq!(5, body["item"]["vendor"]["rating_scale"]);
    assert_eq!(
        0,
        body["item"]["templates"]
            .as_array()
            .expect("templates should be an array")
            .len()
    );
}

#[tokio::test]
async fn template_detail_includes_vendor_slug_from_vendor_profile() {
    // Given a vendor with an approved template
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let vendor = seed_vendor_page(&app, "acme-cloud").await;

    // When fetching the template detail for an approved template owned by that vendor
    let response = reqwest::Client::new()
        .get(format!("{}/api/templates/wordpress-pro", app.address))
        .send()
        .await
        .expect("Failed to fetch template detail");

    // Then the response includes the vendor's public_slug as vendor_slug
    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("template detail response should be valid JSON");

    assert_eq!("OK", body["message"]);
    assert_eq!(
        vendor.public_slug,
        body["item"]["template"]["vendor_slug"]
            .as_str()
            .expect("vendor_slug should be a non-null string")
    );
    assert_eq!(
        "WordPress Pro",
        body["item"]["template"]["name"]
            .as_str()
            .expect("name should be a string")
    );
}

#[tokio::test]
async fn template_detail_vendor_slug_is_null_when_no_vendor_profile() {
    // Given a template whose creator has no marketplace_vendor_profile record
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    // Insert a template directly without creating a vendor profile
    sqlx::query(
        r#"INSERT INTO stack_template (
            id, creator_user_id, creator_name, name, slug, status,
            short_description, tags, tech_stack
        )
        VALUES (
            'a0000000-0000-0000-0000-000000000001'::uuid,
            'orphan_creator',
            'Orphan Creator',
            'Orphan Template',
            'orphan-template',
            'approved',
            'A template with no vendor profile',
            '[]'::jsonb,
            '{}'::jsonb
        )"#,
    )
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert orphan template");

    // When fetching the template detail
    let response = reqwest::Client::new()
        .get(format!("{}/api/templates/orphan-template", app.address))
        .send()
        .await
        .expect("Failed to fetch orphan template detail");

    // Then vendor_slug is null because no vendor profile exists
    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("template detail response should be valid JSON");

    assert!(body["item"]["template"]["vendor_slug"].is_null());
    assert_eq!(
        "Orphan Template",
        body["item"]["template"]["name"]
            .as_str()
            .expect("name should be a string")
    );
}
