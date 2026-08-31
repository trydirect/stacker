use cucumber::given;
use cucumber::when;
use serde_json::json;

use crate::steps::StepWorld;

fn store_template_id(world: &mut StepWorld) {
    if let Some(json) = &world.response_json {
        // Template responses use "data" wrapper
        if let Some(id) = json
            .pointer("/data/id")
            .or_else(|| json.pointer("/item/id"))
            .and_then(|v| {
                v.as_str()
                    .or_else(|| v.as_i64().map(|_| ""))
                    .and_then(|_| v.as_str())
            })
        {
            world
                .stored_ids
                .insert("template_id".to_string(), id.to_string());
        }
        // Also try as integer
        if !world.stored_ids.contains_key("template_id") {
            if let Some(id) = json
                .pointer("/data/id")
                .or_else(|| json.pointer("/item/id"))
                .and_then(|v| v.as_i64())
            {
                world
                    .stored_ids
                    .insert("template_id".to_string(), id.to_string());
            }
        }
    }
}

fn template_create_body(slug: &str, source_project_id: i32) -> serde_json::Value {
    json!({
        "name": format!("BDD Test Template {}", slug),
        "slug": slug,
        "short_description": "A BDD test template",
        "long_description": "This template is created for BDD testing purposes.",
        "category_code": "Dev Tools",
        "tags": ["bdd", "test"],
        "version": "1.0.0",
        "stack_definition": {
            "version": "3.8",
            "services": {
                "web": {
                    "image": "nginx:alpine",
                    "ports": ["80:80"]
                }
            }
        },
        "definition_format": "yaml",
        "plan_type": "free",
        "source_project_id": source_project_id
    })
}

/// Seed a project with a successful ("running") deployment and return its
/// id, so BDD-created templates satisfy the submit/resubmit deployment gate
/// (`ensure_has_successful_deployment` in `routes/marketplace/creator.rs`) —
/// a template can't be submitted for review without one.
async fn seed_source_project(world: &StepWorld) -> i32 {
    let pool = world.db_pool.as_ref().expect("no db_pool");
    let deployment_hash = format!("bdd-marketplace-src-{}", uuid::Uuid::new_v4());
    let proj_name = format!("bdd-marketplace-src-{}", uuid::Uuid::new_v4());

    let project_id: i32 = sqlx::query_scalar(
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
           VALUES (gen_random_uuid(), $1, $2, '{}'::json, '{}'::json, NOW(), NOW())
           RETURNING id"#,
    )
    .bind(super::common::USER_A_ID)
    .bind(&proj_name)
    .fetch_one(pool)
    .await
    .expect("Failed to seed source project for marketplace template");

    sqlx::query(
        r#"INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, runtime, created_at, updated_at)
           VALUES ($1, $2, $3, '{}'::jsonb, 'running', 'runc', NOW(), NOW())"#,
    )
    .bind(project_id)
    .bind(&deployment_hash)
    .bind(super::common::USER_A_ID)
    .execute(pool)
    .await
    .expect("Failed to seed source deployment for marketplace template");

    project_id
}

// ─── Creator steps ───

#[when(regex = r#"^I create a marketplace template with slug "([^"]+)"$"#)]
async fn create_template(world: &mut StepWorld, slug: String) {
    let source_project_id = seed_source_project(world).await;
    let body = template_create_body(&slug, source_project_id);
    world.post_json("/api/templates", &body).await;
    store_template_id(world);
}

#[given(regex = r#"^I have created a marketplace template with slug "([^"]+)"$"#)]
async fn given_created_template(world: &mut StepWorld, slug: String) {
    let source_project_id = seed_source_project(world).await;
    let body = template_create_body(&slug, source_project_id);
    world.post_json("/api/templates", &body).await;
    store_template_id(world);
}

#[when(regex = r#"^I update the stored template with name "([^"]+)"$"#)]
async fn update_template(world: &mut StepWorld, name: String) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let body = json!({ "name": name });
    world
        .put_json(&format!("/api/templates/{}", id), &body)
        .await;
}

#[when("I submit the stored template for review")]
async fn submit_template(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    world
        .post_json(
            &format!("/api/templates/{}/submit", id),
            &json!({ "confirm_no_secrets": true }),
        )
        .await;
}

#[given("I submit the stored template for review")]
async fn given_submit_template(world: &mut StepWorld) {
    submit_template(world).await;
}

#[given(regex = r#"^I submit template "([^"]+)" for review$"#)]
async fn given_submit_template_by_slug(world: &mut StepWorld, _slug: String) {
    // Template was already stored by slug creation; submit by stored ID
    submit_template(world).await;
}

#[when("I list my marketplace templates")]
async fn list_my_templates(world: &mut StepWorld) {
    world.get("/api/templates/mine").await;
}

#[when(regex = r#"^I resubmit the stored template with version "([^"]+)"$"#)]
async fn resubmit_template(world: &mut StepWorld, version: String) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let body = json!({
        "version": version,
        "stack_definition": {
            "version": "3.8",
            "services": {
                "web": {
                    "image": "nginx:latest",
                    "ports": ["80:80", "443:443"]
                }
            }
        },
        "changelog": "Updated for BDD resubmit test",
        "confirm_no_secrets": true
    });
    world
        .post_json(&format!("/api/templates/{}/resubmit", id), &body)
        .await;
}

#[when("I get my vendor profile")]
async fn get_vendor_profile(world: &mut StepWorld) {
    world.get("/api/templates/mine/vendor-profile").await;
}

#[when("I create vendor onboarding link")]
async fn create_onboarding_link(world: &mut StepWorld) {
    world
        .post_json(
            "/api/templates/mine/vendor-profile/onboarding-link",
            &json!({}),
        )
        .await;
}

// ─── Admin steps ───

#[when("I list submitted templates")]
async fn list_submitted(world: &mut StepWorld) {
    world.get("/api/admin/templates").await;
}

#[when("I get admin template detail for the stored template")]
async fn get_admin_detail(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    world.get(&format!("/api/admin/templates/{}", id)).await;
}

#[when("I approve the stored template")]
async fn approve_template(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let body = json!({ "decision": "approved", "reason": "BDD test approval" });
    world
        .post_json(&format!("/api/admin/templates/{}/approve", id), &body)
        .await;
}

#[given("I approve the stored template")]
async fn given_approve_template(world: &mut StepWorld) {
    approve_template(world).await;
}

#[when(regex = r#"^I reject the stored template with reason "([^"]+)"$"#)]
async fn reject_template(world: &mut StepWorld, reason: String) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let body = json!({ "decision": "rejected", "reason": reason });
    world
        .post_json(&format!("/api/admin/templates/{}/reject", id), &body)
        .await;
}

#[when(regex = r#"^I request changes for the stored template with reason "([^"]+)"$"#)]
async fn needs_changes(world: &mut StepWorld, reason: String) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let body = json!({ "reason": reason });
    world
        .post_json(&format!("/api/admin/templates/{}/needs-changes", id), &body)
        .await;
}

#[given(regex = r#"^I request changes for the stored template with reason "([^"]+)"$"#)]
async fn given_needs_changes(world: &mut StepWorld, reason: String) {
    needs_changes(world, reason).await;
}

#[when("I run security scan for the stored template")]
async fn security_scan(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    world
        .post_json(
            &format!("/api/admin/templates/{}/security-scan", id),
            &json!({}),
        )
        .await;
}

#[when(
    regex = r#"^I update pricing for the stored template with price ([0-9.]+) and billing "([^"]+)"$"#
)]
async fn update_pricing(world: &mut StepWorld, price: f64, billing: String) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let body = json!({
        "price": price,
        "billing_cycle": billing
    });
    world
        .patch(&format!("/api/admin/templates/{}/pricing", id), &body)
        .await;
}

#[when(regex = r#"^I update verifications for the stored template with security_reviewed true$"#)]
async fn update_verifications(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let body = json!({ "security_reviewed": true });
    world
        .patch(&format!("/api/admin/templates/{}/verifications", id), &body)
        .await;
}

#[when("I unapprove the stored template")]
async fn unapprove_template(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    world
        .post_json(
            &format!("/api/admin/templates/{}/unapprove", id),
            &json!({}),
        )
        .await;
}

// ─── Public steps ───

#[when("I list marketplace categories")]
async fn list_categories(world: &mut StepWorld) {
    world.get("/api/categories").await;
}

#[when("I list marketplace templates")]
async fn list_templates(world: &mut StepWorld) {
    world.get("/api/templates").await;
}

#[when(regex = r#"^I get marketplace template by slug "([^"]+)"$"#)]
async fn get_template_by_slug(world: &mut StepWorld, slug: String) {
    world.get(&format!("/api/templates/{}", slug)).await;
}

// ─── Analytics setup steps ───

#[given("the template has usage metrics")]
async fn given_template_has_usage_metrics(world: &mut StepWorld) {
    // Insert mock usage metrics into the database for the stored template
    let template_id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let pool = world.db_pool.as_ref().unwrap();

    // Insert mock view events
    let _ = sqlx::query(
        r#"INSERT INTO marketplace_template_event 
           (template_id, event_type, user_id, created_at)
           VALUES ($1::uuid, 'view', 'viewer-1', NOW() - INTERVAL '5 days'),
                  ($1::uuid, 'view', 'viewer-2', NOW() - INTERVAL '3 days'),
                  ($1::uuid, 'view', 'viewer-3', NOW() - INTERVAL '1 day')"#,
    )
    .bind(&template_id)
    .execute(pool)
    .await;

    // Insert mock deployment events
    let _ = sqlx::query(
        r#"INSERT INTO marketplace_template_event 
           (template_id, event_type, user_id, cloud_provider, created_at)
           VALUES ($1::uuid, 'deploy', 'deployer-1', 'hetzner', NOW() - INTERVAL '4 days'),
                  ($1::uuid, 'deploy', 'deployer-2', 'digitalocean', NOW() - INTERVAL '2 days')"#,
    )
    .bind(&template_id)
    .execute(pool)
    .await;
}

#[given("the template has usage events across periods")]
async fn given_template_has_usage_events_across_periods(world: &mut StepWorld) {
    // Insert events across multiple time periods for period filtering test
    let template_id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let pool = world.db_pool.as_ref().unwrap();

    // Events in last 7 days
    let _ = sqlx::query(
        r#"INSERT INTO marketplace_template_event 
           (template_id, event_type, user_id, created_at)
           VALUES ($1::uuid, 'view', 'viewer-recent-1', NOW() - INTERVAL '2 days'),
                  ($1::uuid, 'view', 'viewer-recent-2', NOW() - INTERVAL '5 days')"#,
    )
    .bind(&template_id)
    .execute(pool)
    .await;

    // Events beyond 7 days but within 30 days
    let _ = sqlx::query(
        r#"INSERT INTO marketplace_template_event 
           (template_id, event_type, user_id, created_at)
           VALUES ($1::uuid, 'view', 'viewer-old-1', NOW() - INTERVAL '15 days'),
                  ($1::uuid, 'view', 'viewer-old-2', NOW() - INTERVAL '25 days')"#,
    )
    .bind(&template_id)
    .execute(pool)
    .await;
}

// ─── Analytics request steps ───

#[when(regex = r#"^I request my marketplace analytics for period "([^"]+)"$"#)]
async fn request_my_marketplace_analytics(world: &mut StepWorld, period: String) {
    world
        .get(&format!("/api/templates/mine/analytics?period={}", period))
        .await;
}

#[when("I request analytics for User A template scope")]
async fn request_analytics_for_user_a_template_scope(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    world
        .get(&format!(
            "/api/templates/mine/analytics?period=30d&templateId={template_id}"
        ))
        .await;
}
