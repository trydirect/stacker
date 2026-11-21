//! Regression for BUGS.md #28: `stacker install` and the deploy that follows
//! must report the same project id for the same project.
//!
//! Installing under a name a project already had inserted a *second* project
//! row, and the deploy's name lookup then resolved the older one — two
//! different ids inside one run. A project's identity is (user, name), the way
//! the deploy chain's resolve-or-create already treats it, so a repeat
//! install must land on the row the next lookup resolves.

mod common;

use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// App whose User Service answers the plan gate a marketplace install checks:
/// `user_has_plan("professional")` reads `/oauth_server/api/me`.
async fn spawn_install_app() -> Option<common::TestApp> {
    let user_service = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/oauth_server/api/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user": { "plan": { "code": "plan-pro-monthly" } }
        })))
        .mount(&user_service)
        .await;

    let mut configuration =
        stacker::configuration::get_configuration().expect("Failed to get configuration");
    configuration.user_service_url = user_service.uri();
    // The mock must outlive the app, which lives for the rest of the test.
    std::mem::forget(user_service);

    common::spawn_app_with_test_auth_configuration(configuration).await
}

async fn create_installable_template(app: &common::TestApp, slug: &str) {
    let created = Client::new()
        .post(format!("{}/api/templates", app.address))
        .bearer_auth(common::USER_A_TOKEN)
        .json(&json!({
            "name": "Install Identity Template",
            "slug": slug,
            "version": "1.0.0",
            "stack_definition": { "services": { "web": { "image": "nginx:1.28" } } },
        }))
        .send()
        .await
        .expect("Failed to create template");
    assert_eq!(
        StatusCode::CREATED,
        created.status(),
        "template creation should succeed"
    );

    // The install route resolves approved templates only; the review workflow
    // is not this test's subject.
    sqlx::query("UPDATE stack_template SET status = 'approved' WHERE slug = $1")
        .bind(slug)
        .execute(&app.db_pool)
        .await
        .expect("Failed to approve template");
}

async fn install(app: &common::TestApp, slug: &str, name: &str) -> Value {
    let response = Client::new()
        .post(format!("{}/api/templates/{}/install", app.address, slug))
        .bearer_auth(common::USER_A_TOKEN)
        .json(&json!({ "name": name }))
        .send()
        .await
        .expect("Failed to send install request");
    assert_eq!(
        StatusCode::OK,
        response.status(),
        "install of '{slug}' as '{name}' should succeed"
    );
    let body: Value = response
        .json()
        .await
        .expect("Install response should be valid JSON");
    body["item"].clone()
}

#[tokio::test]
async fn installing_twice_under_one_name_reports_one_project_id() {
    let Some(app) = spawn_install_app().await else {
        return;
    };
    let slug = "install-identity-slug";
    create_installable_template(&app, slug).await;

    let first = install(&app, slug, "My N8n").await;
    let first_id = first["project"]["id"].clone();
    assert!(
        first_id.is_number(),
        "install should report its project: {first}"
    );

    // The same identity under a different spelling: `sanitize_project_name`
    // maps both to `my_n8n`, and the deploy chain resolves the project by
    // exactly that name.
    let second = install(&app, slug, "my n8n").await;
    assert_eq!(
        first_id, second["project"]["id"],
        "a repeat install under one project name must report the same project id"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project WHERE name = 'my_n8n'")
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to count projects");
    assert_eq!(1, count, "one project row per (user, name)");
}

#[tokio::test]
async fn installing_under_a_different_name_keeps_a_separate_project() {
    let Some(app) = spawn_install_app().await else {
        return;
    };
    let slug = "install-identity-other-slug";
    create_installable_template(&app, slug).await;

    let first = install(&app, slug, "first-instance").await;
    let second = install(&app, slug, "second-instance").await;

    assert_ne!(
        first["project"]["id"], second["project"]["id"],
        "different project names are different projects"
    );
}
