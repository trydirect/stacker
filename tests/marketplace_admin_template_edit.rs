mod common;

use chrono::{Duration, Utc};
use reqwest::StatusCode;
use serde_json::json;
use sqlx::Row;
use tokio::sync::OnceCell;

/// `PUT /api/admin/templates/{id}` - an admin correcting a published listing.
///
/// The author's own `PUT /api/templates/{id}` refuses anything outside
/// draft/rejected/needs_changes, because it models the review workflow. An
/// admin fixing the copy on an approved listing is not in that workflow, so
/// the admin route lifts the status gate while reusing the same write path
/// (`creator::apply_template_update`) for validation and secret stripping.
///
/// Who may call the route is covered by `security_route_matrix.rs`, which
/// classifies every `/api/admin/*` path as admin-only and checks live that a
/// group_user is refused.

// A fresh `PgPool` per test, bound to that test's own runtime - see the note in
// tests/marketplace_admin_review.rs.
static APP_CONFIG: OnceCell<common::TestAppConfig> = OnceCell::const_new();

async fn app() -> common::TestApp {
    common::get_or_init_app_fresh(&APP_CONFIG)
        .await
        .expect("Failed to start test app")
}

fn create_admin_jwt() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let header = json!({"alg": "HS256", "typ": "JWT"});
    let payload = json!({
        "role": "admin_service",
        "email": "ops@test.com",
        "exp": (Utc::now() + Duration::minutes(30)).timestamp(),
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string());
    let signature = common::sign_test_jwt(&header_b64, &payload_b64);
    format!("{}.{}.{}", header_b64, payload_b64, signature)
}

async fn insert_template(pool: &sqlx::PgPool, slug: &str, status: &str) -> String {
    sqlx::query(
        r#"INSERT INTO stack_template (
            creator_user_id, creator_name, name, slug, status,
            short_description, long_description, tags, tech_stack
        )
        VALUES ($1, 'Creator Example', 'Original Name', $2, $3,
            'original short', 'original long', '[]'::jsonb, '{}'::jsonb)
        RETURNING id"#,
    )
    .bind(common::USER_A_ID)
    .bind(slug)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("Failed to insert template")
    .get::<uuid::Uuid, _>("id")
    .to_string()
}

struct StoredTemplate {
    name: String,
    short_description: Option<String>,
    long_description: Option<String>,
    status: String,
}

async fn fetch_template(pool: &sqlx::PgPool, id: &str) -> StoredTemplate {
    let row = sqlx::query(
        r#"SELECT name, short_description, long_description, status
           FROM stack_template WHERE id = $1::uuid"#,
    )
    .bind(uuid::Uuid::parse_str(id).expect("template id should be a uuid"))
    .fetch_one(pool)
    .await
    .expect("Failed to fetch template");

    StoredTemplate {
        name: row.get("name"),
        short_description: row.get("short_description"),
        long_description: row.get("long_description"),
        status: row.get("status"),
    }
}

/// The long description is authored in the admin UI's Draft editor, so what
/// arrives is raw Draft JSON serialized into a text column. It has to survive
/// the round trip byte for byte: the storefront decides how to render by
/// parsing it, and a mangled payload silently degrades to markdown.
const DRAFT_JSON: &str = r#"{"blocks":[{"key":"a1","text":"Overview","type":"header-two","depth":0,"inlineStyleRanges":[],"entityRanges":[],"data":{}},{"key":"a2","text":"Runs anywhere.","type":"unstyled","depth":0,"inlineStyleRanges":[{"offset":0,"length":4,"style":"BOLD"}],"entityRanges":[],"data":{}}],"entityMap":{}}"#;

#[tokio::test]
async fn admin_can_edit_an_approved_template() {
    let app = app().await;
    let template_id = insert_template(&app.db_pool, "admin-edit-approved", "approved").await;

    let response = reqwest::Client::new()
        .put(format!(
            "{}/api/admin/templates/{}",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "name": "Corrected Name",
            "short_description": "corrected short",
            "long_description": DRAFT_JSON,
        }))
        .send()
        .await
        .expect("Failed to send admin edit request");

    assert_eq!(StatusCode::OK, response.status());

    let stored = fetch_template(&app.db_pool, &template_id).await;
    assert_eq!("Corrected Name", stored.name);
    assert_eq!(Some("corrected short".to_string()), stored.short_description);
    assert_eq!(Some(DRAFT_JSON.to_string()), stored.long_description);
    // An edit is not a review decision.
    assert_eq!("approved", stored.status);
}

#[tokio::test]
async fn the_author_cannot_edit_an_approved_template_through_their_own_route() {
    // The contrast that justifies the admin route: same fixture, same payload,
    // refused on the creator path because the review workflow owns the status.
    let app = app().await;
    let template_id = insert_template(&app.db_pool, "author-edit-approved", "approved").await;

    let response = reqwest::Client::new()
        .put(format!("{}/api/templates/{}", app.address, template_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({"name": "Author Rename"}))
        .send()
        .await
        .expect("Failed to send creator edit request");

    assert_ne!(StatusCode::OK, response.status());

    let stored = fetch_template(&app.db_pool, &template_id).await;
    assert_eq!("Original Name", stored.name);
}

#[tokio::test]
async fn admin_edit_leaves_omitted_fields_alone() {
    let app = app().await;
    let template_id = insert_template(&app.db_pool, "admin-edit-partial", "approved").await;

    let response = reqwest::Client::new()
        .put(format!(
            "{}/api/admin/templates/{}",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({"short_description": "only this one changes"}))
        .send()
        .await
        .expect("Failed to send admin edit request");

    assert_eq!(StatusCode::OK, response.status());

    let stored = fetch_template(&app.db_pool, &template_id).await;
    assert_eq!(
        Some("only this one changes".to_string()),
        stored.short_description
    );
    assert_eq!("Original Name", stored.name);
    assert_eq!(Some("original long".to_string()), stored.long_description);
}

#[tokio::test]
async fn admin_edit_of_a_missing_template_is_not_found() {
    let app = app().await;
    let missing = uuid::Uuid::new_v4();

    let response = reqwest::Client::new()
        .put(format!("{}/api/admin/templates/{}", app.address, missing))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({"name": "Nobody"}))
        .send()
        .await
        .expect("Failed to send admin edit request");

    assert_eq!(StatusCode::NOT_FOUND, response.status());
}

#[tokio::test]
async fn admin_can_edit_a_draft_template_too() {
    // The status gate is lifted, not inverted: the statuses the author may
    // edit must stay editable for an admin as well.
    let app = app().await;
    let template_id = insert_template(&app.db_pool, "admin-edit-draft", "draft").await;

    let response = reqwest::Client::new()
        .put(format!(
            "{}/api/admin/templates/{}",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({"name": "Draft Corrected"}))
        .send()
        .await
        .expect("Failed to send admin edit request");

    assert_eq!(StatusCode::OK, response.status());
    let stored = fetch_template(&app.db_pool, &template_id).await;
    assert_eq!("Draft Corrected", stored.name);
    assert_eq!("draft", stored.status);
}
