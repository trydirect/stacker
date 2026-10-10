mod common;

use actix_web::web;
use serde_json::{json, Map, Value};
use stacker::mcp::protocol::ToolContent;
use stacker::mcp::{ToolContext, ToolRegistry};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// IDOR sweep over every MCP tool a regular user may call.
///
/// The tool list is not written here: it comes from `ToolRegistry::new()` and
/// from the Casbin rules (`/mcp/tools/<name>`, action CALL) granted to
/// `group_user` by the migrations. Every such tool is called as User B with
/// arguments that point at User A's project, deployment, cloud and server,
/// built from the tool's own input schema.
///
/// A tool passes when it refuses the call. It fails when it answers with
/// data, when it is still running after a few seconds (it queued a command
/// for the agent and is waiting for the result), or when User A's data or
/// command queue changed afterwards.
///
/// Tools whose schema takes no resource reference act on the caller's own
/// data; they must be listed in `NO_RESOURCE_TOOLS`, so a new tool cannot
/// skip the sweep by accident.

const DEPLOYMENT_HASH: &str = "mcp-sweep-deployment-hash";
const APP_CODE: &str = "web";
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Argument names that refer to a resource owned by somebody.
const RESOURCE_ARGS: &[&str] = &[
    "id",
    "project_id",
    "deployment_hash",
    "deployment_id",
    "installation_id",
    "cloud_id",
    "server_id",
    "instance_id",
    "pipe_id",
    "template_id",
];

/// Tools a regular user may call that take no reference to another user's
/// resource: they list, search or act on the caller's own account.
const NO_RESOURCE_TOOLS: &[&str] = &[
    "add_cloud",
    "create_project",
    // A stub: validates the role and answers "queued" without running anything.
    "deploy_role",
    "escalate_to_support",
    "get_ansible_role_defaults",
    "get_live_chat_info",
    "get_notifications",
    "get_role_details",
    "get_role_requirements",
    "get_subscription_plan",
    "get_user_profile",
    // Forwards its payload to User Service under the caller's own token.
    "initiate_deployment",
    "list_available_roles",
    "list_clouds",
    "list_installations",
    "list_projects",
    "list_templates",
    "mark_all_notifications_read",
    "mark_notification_read",
    "preview_install_config",
    "recommend_stack_services",
    "render_ansible_template",
    "search_applications",
    "search_marketplace_templates",
    "suggest_resources",
    "validate_domain",
    "validate_role_vars",
];

/// Tools that take a resource reference and were checked by reading the code:
/// they hand it, with the caller's own token, to a service that checks
/// ownership itself (or ignores it), or they only act for the caller.
const REVIEWED_TOOLS: &[(&str, &str)] = &[
    (
        "escalate_to_support",
        "opens a ticket for the caller; adds deployment details only when the caller owns it",
    ),
    (
        "add_app_to_deployment",
        "User Service filters installations by owner",
    ),
    (
        "get_installation_details",
        "User Service filters installations by owner",
    ),
    (
        "trigger_redeploy",
        "User Service filters installations by owner",
    ),
    (
        "list_cloud_images",
        "App Service ignores cloud_id: public provider catalog",
    ),
    (
        "list_cloud_regions",
        "App Service ignores cloud_id: public provider catalog",
    ),
    (
        "list_cloud_server_sizes",
        "App Service ignores cloud_id: public provider catalog",
    ),
];

struct Fixture {
    project_id: i32,
    deployment_id: i32,
    cloud_id: i32,
    server_id: i32,
}

fn user(id: &str, email: &str) -> Arc<stacker::models::User> {
    Arc::new(stacker::models::User {
        id: id.to_string(),
        first_name: "Test".to_string(),
        last_name: "User".to_string(),
        email: email.to_string(),
        role: "group_user".to_string(),
        email_confirmed: true,
        mfa_verified: true,
        access_token: Some(common::USER_B_TOKEN.to_string()),
    })
}

/// Build arguments for `tool` from its input schema, pointing every resource
/// reference at User A's fixture.
fn arguments(tool: &str, schema: &Value, fx: &Fixture) -> Value {
    let props = schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let required: BTreeSet<String> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let has_hash = props.contains_key("deployment_hash");
    let has_app_code = props.contains_key("app_code");

    let mut args = Map::new();
    for (name, prop) in &props {
        let value = match name.as_str() {
            "project_id" => json!(fx.project_id),
            "id" if tool.contains("cloud") => json!(fx.cloud_id),
            "id" => json!(fx.project_id),
            "deployment_hash" => json!(DEPLOYMENT_HASH),
            // With a hash present, the id is the legacy installation id and
            // would send the tool to User Service instead.
            "deployment_id" | "installation_id" if has_hash => continue,
            "deployment_id" | "installation_id" => json!(fx.deployment_id),
            "cloud_id" => json!(fx.cloud_id),
            "server_id" => json!(fx.server_id),
            "code" if has_app_code => continue,
            "domain_names" => json!(["intruder.example.com"]),
            "domain" => json!("intruder.example.com"),
            "confirm" => json!(true),
            "app_code" | "code" | "app" | "service" | "service_name" | "container_name"
            | "app_name" | "container" => json!(APP_CODE),
            _ if !required.contains(name) => continue,
            _ => filler(prop),
        };
        args.insert(name.clone(), value);
    }
    Value::Object(args)
}

fn filler(prop: &Value) -> Value {
    if let Some(first) = prop
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|e| e.first())
    {
        return first.clone();
    }
    match prop.get("type").and_then(Value::as_str) {
        Some("integer") | Some("number") => json!(1),
        Some("boolean") => json!(false),
        Some("array") => json!([]),
        Some("object") => json!({}),
        _ => json!("x"),
    }
}

fn takes_resource(schema: &Value) -> bool {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|p| p.keys().any(|k| RESOURCE_ARGS.contains(&k.as_str())))
        .unwrap_or(false)
}

/// Tools `group_user` may call, according to the Casbin rules in the database
/// (including roles `group_user` inherits through `g` rules).
async fn tools_granted_to_group_user(pool: &sqlx::PgPool) -> BTreeSet<String> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"WITH RECURSIVE roles(name) AS (
               SELECT 'group_user'::text
               UNION
               SELECT g.v1 FROM casbin_rule g JOIN roles r ON g.ptype = 'g' AND g.v0 = r.name
           )
           SELECT DISTINCT substring(p.v1 from '^/mcp/tools/(.+)$')
           FROM casbin_rule p JOIN roles r ON p.ptype = 'p' AND p.v0 = r.name
           WHERE p.v1 LIKE '/mcp/tools/%' AND p.v2 = 'CALL'"#,
    )
    .fetch_all(pool)
    .await
    .expect("casbin_rule query failed");
    rows.into_iter().map(|r| r.0).collect()
}

async fn snapshot(pool: &sqlx::PgPool, fx: &Fixture) -> String {
    let queries = [
        ("project", "SELECT row_to_json(t)::text FROM (SELECT user_id, name, request_json, metadata FROM project WHERE id = $1) t"),
        ("apps", "SELECT COALESCE(json_agg(t ORDER BY t.code)::text, '[]') FROM (SELECT code, image, environment, ports, domain, enabled FROM project_app WHERE project_id = $1) t"),
        ("deployment", "SELECT row_to_json(t)::text FROM (SELECT user_id, status, metadata FROM deployment WHERE project_id = $1) t"),
        ("server", "SELECT COALESCE(json_agg(t)::text, '[]') FROM (SELECT * FROM server WHERE project_id = $1) t"),
    ];
    let mut out = String::new();
    for (label, sql) in queries {
        let row: Option<(Option<String>,)> = sqlx::query_as(sql)
            .bind(fx.project_id)
            .fetch_optional(pool)
            .await
            .unwrap_or_else(|e| panic!("snapshot {label}: {e}"));
        out.push_str(&format!("{label}={:?}\n", row.and_then(|r| r.0)));
    }
    let cloud: Option<(Option<String>,)> =
        sqlx::query_as("SELECT row_to_json(t)::text FROM (SELECT * FROM cloud WHERE id = $1) t")
            .bind(fx.cloud_id)
            .fetch_optional(pool)
            .await
            .unwrap();
    out.push_str(&format!("cloud={:?}\n", cloud.and_then(|r| r.0)));
    out
}

#[tokio::test]
async fn other_user_cannot_use_mcp_tools_on_someone_elses_resources() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let pool = &app.db_pool;

    // User A: a project with an app, a running deployment with an online
    // agent, a saved cloud and a server.
    let project_id = common::create_test_project(pool, common::USER_A_ID).await;
    sqlx::query(
        "INSERT INTO project_app (project_id, code, name, image, environment, created_at, updated_at)
         VALUES ($1, $2, $2, 'nginx:alpine', '{\"OWNER_ONLY\":\"a-secret\"}'::jsonb, NOW(), NOW())",
    )
    .bind(project_id)
    .bind(APP_CODE)
    .execute(pool)
    .await
    .expect("Failed to insert project_app");
    let deployment_id =
        common::create_test_deployment(pool, common::USER_A_ID, project_id, DEPLOYMENT_HASH).await;
    sqlx::query(
        "INSERT INTO agents (deployment_hash, capabilities, status, last_heartbeat, created_at, updated_at)
         VALUES ($1, $2, 'online', NOW(), NOW(), NOW())",
    )
    .bind(DEPLOYMENT_HASH)
    .bind(json!(["docker", "compose", "logs", "proxy", "firewall", "pipes", "kata"]))
    .execute(pool)
    .await
    .expect("Failed to insert agent");
    let cloud_id = common::create_test_cloud(pool, common::USER_A_ID, "a-mcp", "htz").await;
    let server_id =
        common::create_test_server(pool, common::USER_A_ID, project_id, "none", None).await;
    let fx = Fixture {
        project_id,
        deployment_id,
        cloud_id,
        server_id,
    };
    let before = snapshot(pool, &fx).await;

    let registry = ToolRegistry::new();
    let granted = tools_granted_to_group_user(pool).await;
    assert!(
        granted.len() > 20,
        "only {} MCP tools granted to group_user - did the casbin migrations run?",
        granted.len()
    );

    // User Service, Vault and App Service answer as if everything existed, so
    // that a tool cannot be refused by a failed network call before its
    // ownership check is reached.
    let external = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/role"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "name": "x", "public_ports": ["8080"], "private_ports": [] }
        ])))
        .mount(&external)
        .await;
    Mock::given(path_regex("^/v1/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "data": {
                "content": "services: {}",
                "content_type": "text/yaml",
                "destination_path": "/opt/stack/docker-compose.yml"
            } }
        })))
        .mount(&external)
        .await;
    Mock::given(path_regex(".*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&external)
        .await;
    let mut settings = stacker::configuration::Settings::default();
    settings.user_service_url = external.uri();
    settings.vault.address = external.uri();

    let context = ToolContext {
        user: user(common::USER_B_ID, common::USER_B_EMAIL),
        pg_pool: pool.clone(),
        settings: web::Data::new(settings),
    };

    let mut failures = Vec::new();
    let mut unreviewed = Vec::new();
    for name in &granted {
        let Some(handler) = registry.get(name) else {
            continue; // a rule for a tool that is no longer registered
        };
        let schema = handler.schema().input_schema;
        if !takes_resource(&schema) {
            if !NO_RESOURCE_TOOLS.contains(&name.as_str()) {
                unreviewed.push(name.clone());
            }
            continue;
        }
        if REVIEWED_TOOLS.iter().any(|(tool, _)| tool == name) {
            continue;
        }
        let args = arguments(name, &schema, &fx);
        match tokio::time::timeout(CALL_TIMEOUT, handler.execute(args.clone(), &context)).await {
            Err(_) => failures.push(format!(
                "IDOR  {name} {args}: still running after {:?} - it is waiting on User A's agent",
                CALL_TIMEOUT
            )),
            Ok(Ok(content)) => {
                let text = match content {
                    ToolContent::Text { text } => text,
                    other => format!("{:?}", other),
                };
                failures.push(format!(
                    "IDOR  {name} {args}: answered {}",
                    text.chars().take(300).collect::<String>()
                ));
            }
            Ok(Err(refused)) => eprintln!("refused {name} {args}: {refused}"),
        }
    }

    let queued: Vec<(String, String)> = sqlx::query_as(
        "SELECT command_id, type FROM commands WHERE deployment_hash = $1 AND created_by = $2",
    )
    .bind(DEPLOYMENT_HASH)
    .bind(common::USER_B_ID)
    .fetch_all(pool)
    .await
    .unwrap();
    for (command_id, kind) in &queued {
        failures.push(format!(
            "QUEUE User B queued '{kind}' ({command_id}) for User A's deployment"
        ));
    }
    let after = snapshot(pool, &fx).await;
    if before != after {
        failures.push(format!(
            "DATA  User A's data changed:\nbefore:\n{before}after:\n{after}"
        ));
    }

    assert!(
        unreviewed.is_empty(),
        "MCP tools granted to group_user take no known resource argument - check them and add \
         to NO_RESOURCE_TOOLS or RESOURCE_ARGS: {:?}",
        unreviewed
    );
    assert!(
        failures.is_empty(),
        "{} MCP problem(s) - another user reached User A's resources:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
