mod common;

use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::Row;

const USER_TOKEN: &str = "test-bearer-token";

async fn seed_template(app: &common::TestApp, slug: &str) -> uuid::Uuid {
    let vendor = common::seed_marketplace_vendor_fixture(&app.db_pool, "acme-cloud").await;
    common::seed_marketplace_template_fixtures_for_vendor(&app.db_pool, &vendor.creator_user_id)
        .await;

    sqlx::query("SELECT id FROM stack_template WHERE slug = $1")
        .bind(slug)
        .fetch_one(&app.db_pool)
        .await
        .expect("seeded template should exist")
        .get("id")
}

#[tokio::test]
async fn user_can_rate_template_by_template_id_without_product_id_knowledge() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let template_id = seed_template(&app, "wordpress-pro").await;

    let response = reqwest::Client::new()
        .put(format!(
            "{}/api/templates/{}/rating",
            app.address, template_id
        ))
        .bearer_auth(USER_TOKEN)
        .json(&json!({
            "rating": 5,
            "comment": "Excellent marketplace template"
        }))
        .send()
        .await
        .expect("Failed to rate template");

    assert_eq!(StatusCode::OK, response.status());
    let body: Value = response.json().await.expect("rating response JSON");
    assert_eq!(Some(5.0), body["item"]["rating"].as_f64());
    assert_eq!(5, body["item"]["rating_scale"]);
    assert_eq!("Excellent marketplace template", body["item"]["comment"]);

    let summary = reqwest::Client::new()
        .get(format!(
            "{}/api/templates/{}/rating/summary",
            app.address, template_id
        ))
        .send()
        .await
        .expect("Failed to fetch template rating summary");

    assert_eq!(StatusCode::OK, summary.status());
    let summary: Value = summary.json().await.expect("summary JSON");
    assert_eq!(Some(5.0), summary["item"]["rating"].as_f64());
    assert_eq!(1, summary["item"]["rating_count"]);
    assert_eq!(5, summary["item"]["rating_scale"]);
}

#[tokio::test]
async fn user_rating_upsert_updates_existing_template_rating() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let template_id = seed_template(&app, "wordpress-pro").await;
    let client = reqwest::Client::new();

    let first = client
        .put(format!(
            "{}/api/templates/{}/rating",
            app.address, template_id
        ))
        .bearer_auth(USER_TOKEN)
        .json(&json!({ "rating": 5, "comment": "first" }))
        .send()
        .await
        .expect("Failed to create rating");
    assert_eq!(StatusCode::OK, first.status());

    let second = client
        .put(format!(
            "{}/api/templates/{}/rating",
            app.address, template_id
        ))
        .bearer_auth(USER_TOKEN)
        .json(&json!({ "rating": 4, "comment": "updated" }))
        .send()
        .await
        .expect("Failed to update rating");
    assert_eq!(StatusCode::OK, second.status());

    let body: Value = second.json().await.expect("updated rating JSON");
    assert_eq!(Some(4.0), body["item"]["rating"].as_f64());
    assert_eq!("updated", body["item"]["comment"]);

    let count: i64 = sqlx::query(
        "SELECT COUNT(*)::bigint AS count FROM rating r JOIN stack_template t ON t.product_id = r.obj_id WHERE t.id = $1",
    )
    .bind(template_id)
    .fetch_one(&app.db_pool)
    .await
    .expect("rating count query should work")
    .get("count");
    assert_eq!(1, count);
}

#[tokio::test]
async fn user_can_fetch_and_delete_own_template_rating() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let template_id = seed_template(&app, "wordpress-pro").await;
    let client = reqwest::Client::new();

    let create = client
        .put(format!(
            "{}/api/templates/{}/rating",
            app.address, template_id
        ))
        .bearer_auth(USER_TOKEN)
        .json(&json!({ "rating": 3, "comment": "ok" }))
        .send()
        .await
        .expect("Failed to create rating");
    assert_eq!(StatusCode::OK, create.status());

    let mine = client
        .get(format!(
            "{}/api/templates/{}/rating/me",
            app.address, template_id
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to fetch my rating");
    assert_eq!(StatusCode::OK, mine.status());
    let mine: Value = mine.json().await.expect("my rating JSON");
    assert_eq!(Some(3.0), mine["item"]["rating"].as_f64());

    let delete = client
        .delete(format!(
            "{}/api/templates/{}/rating",
            app.address, template_id
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to delete my rating");
    assert_eq!(StatusCode::OK, delete.status());

    let summary = client
        .get(format!(
            "{}/api/templates/{}/rating/summary",
            app.address, template_id
        ))
        .send()
        .await
        .expect("Failed to fetch summary after delete");
    assert_eq!(StatusCode::OK, summary.status());
    let summary: Value = summary.json().await.expect("summary JSON");
    assert!(summary["item"]["rating"].is_null());
    assert_eq!(0, summary["item"]["rating_count"]);
}

#[tokio::test]
async fn template_rating_rejects_invalid_star_values() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let template_id = seed_template(&app, "wordpress-pro").await;

    let response = reqwest::Client::new()
        .put(format!(
            "{}/api/templates/{}/rating",
            app.address, template_id
        ))
        .bearer_auth(USER_TOKEN)
        .json(&json!({ "rating": 6 }))
        .send()
        .await
        .expect("Failed to submit invalid rating");

    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}
