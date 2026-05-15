use cucumber::{given, then, when};
use serde_json::json;

use super::StepWorld;

// ─── Agent Registration steps ────────────────────────────────────

#[when(regex = r#"^I register an agent with deployment hash "(.+)" and capabilities "(.+)"$"#)]
async fn register_agent_with_caps(world: &mut StepWorld, hash: String, caps: String) {
    let capabilities: Vec<&str> = caps.split(',').collect();
    let body = json!({
        "deployment_hash": hash,
        "agent_version": "1.0.0-bdd",
        "capabilities": capabilities,
        "system_info": { "os": "linux", "arch": "x86_64" }
    });
    register_agent_raw(world, &body).await;
}

#[when(regex = r#"^I register an agent with deployment hash "([^"]*)"$"#)]
async fn register_agent(world: &mut StepWorld, hash: String) {
    let body = json!({
        "deployment_hash": hash,
        "agent_version": "1.0.0-bdd",
        "capabilities": ["docker", "logs"],
        "system_info": { "os": "linux", "arch": "x86_64" }
    });
    register_agent_raw(world, &body).await;
}

async fn register_agent_raw(world: &mut StepWorld, body: &serde_json::Value) {
    let url = format!("{}/api/v1/agent/register", world.base_url);
    let resp = world
        .client
        .post(&url)
        .json(body)
        .send()
        .await
        .expect("Register request failed");

    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    world.status_code = Some(status);
    world.response_body = Some(body.clone());
    world.response_json = serde_json::from_str(&body).ok();

    // Store agent_id and agent_token if present
    if let Some(ref json) = world.response_json {
        if let Some(agent_id) = json.pointer("/data/item/agent_id").and_then(|v| v.as_str()) {
            world
                .stored_ids
                .insert("agent_id".to_string(), agent_id.to_string());
        } else if let Some(agent_id) = json.pointer("/item/agent_id").and_then(|v| v.as_str()) {
            world
                .stored_ids
                .insert("agent_id".to_string(), agent_id.to_string());
        }
        if let Some(token) = json
            .pointer("/data/item/agent_token")
            .and_then(|v| v.as_str())
        {
            world
                .stored_ids
                .insert("agent_token".to_string(), token.to_string());
        } else if let Some(token) = json.pointer("/item/agent_token").and_then(|v| v.as_str()) {
            world
                .stored_ids
                .insert("agent_token".to_string(), token.to_string());
        }
    }
}

#[then("the response should contain an agent_id")]
async fn then_has_agent_id(world: &mut StepWorld) {
    assert!(
        world.stored_ids.contains_key("agent_id"),
        "Response should contain agent_id. Body: {}",
        world.response_body.as_deref().unwrap_or("<none>")
    );
}

#[then("the response should contain an agent_token")]
async fn then_has_agent_token(world: &mut StepWorld) {
    assert!(
        world.stored_ids.contains_key("agent_token"),
        "Response should contain agent_token. Body: {}",
        world.response_body.as_deref().unwrap_or("<none>")
    );
}

// ─── Command steps ───────────────────────────────────────────────

#[when(
    regex = r#"^I create a command for deployment "(.+)" with type "(.+)" and priority "(.+)"$"#
)]
async fn create_command_with_priority(
    world: &mut StepWorld,
    deployment_hash: String,
    cmd_type: String,
    priority: String,
) {
    let body = json!({
        "deployment_hash": deployment_hash,
        "command_type": cmd_type,
        "priority": priority,
        "timeout_seconds": 60
    });
    world.post_json("/api/v1/commands", &body).await;
    store_command_id(world);
}

#[when(regex = r#"^I create a command for deployment "(.+)" with type "(.+)" and parameters$"#)]
async fn create_command_with_params(
    world: &mut StepWorld,
    step: &cucumber::gherkin::Step,
    deployment_hash: String,
    cmd_type: String,
) {
    let mut params = serde_json::Map::new();
    if let Some(table) = step.table.as_ref() {
        for row in table.rows.iter().skip(1) {
            if row.len() >= 2 {
                params.insert(row[0].clone(), json!(row[1].clone()));
            }
        }
    }
    let body = json!({
        "deployment_hash": deployment_hash,
        "command_type": cmd_type,
        "priority": "normal",
        "parameters": params,
        "timeout_seconds": 60
    });
    world.post_json("/api/v1/commands", &body).await;
    store_command_id(world);
}

#[when(regex = r#"^I create a command for deployment "([^"]+)" with type "([^"]+)"$"#)]
#[given(regex = r#"^I have created a command for deployment "([^"]+)" with type "([^"]+)"$"#)]
async fn create_command(world: &mut StepWorld, deployment_hash: String, cmd_type: String) {
    let body = json!({
        "deployment_hash": deployment_hash,
        "command_type": cmd_type,
        "priority": "normal",
        "timeout_seconds": 60
    });
    world.post_json("/api/v1/commands", &body).await;
    store_command_id(world);
}

fn store_command_id(world: &mut StepWorld) {
    if let Some(ref json) = world.response_json {
        // Store command_id (string like "cmd_xxx")
        if let Some(id) = json.pointer("/item/command_id").and_then(|v| v.as_str()) {
            world
                .stored_ids
                .insert("command_id".to_string(), id.to_string());
        }
        // Store UUID id if available (from get response)
        if let Some(id) = json.pointer("/item/id").and_then(|v| v.as_str()) {
            world
                .stored_ids
                .insert("command_uuid".to_string(), id.to_string());
        }
    }
}

#[when(regex = r#"^I list commands for deployment "(.+)" with limit (\d+)$"#)]
async fn list_commands_with_limit(world: &mut StepWorld, deployment_hash: String, limit: u32) {
    world
        .get(&format!(
            "/api/v1/commands/{}?limit={}",
            deployment_hash, limit
        ))
        .await;
}

#[when(regex = r#"^I list commands for deployment "(.+)"$"#)]
async fn list_commands(world: &mut StepWorld, deployment_hash: String) {
    world
        .get(&format!("/api/v1/commands/{}", deployment_hash))
        .await;
}

#[when(regex = r#"^I get the stored command for deployment "(.+)"$"#)]
async fn get_stored_command(world: &mut StepWorld, deployment_hash: String) {
    let cmd_id = world
        .stored_ids
        .get("command_id")
        .expect("No stored command_id")
        .clone();
    world
        .get(&format!("/api/v1/commands/{}/{}", deployment_hash, cmd_id))
        .await;
}

#[when(regex = r#"^I cancel the stored command for deployment "(.+)"$"#)]
async fn cancel_stored_command(world: &mut StepWorld, deployment_hash: String) {
    let cmd_id = world
        .stored_ids
        .get("command_id")
        .expect("No stored command_id")
        .clone();

    // First GET the command to obtain the row UUID (cancel uses fetch_by_id which needs UUID)
    world
        .get(&format!("/api/v1/commands/{}/{}", deployment_hash, cmd_id))
        .await;
    let uuid = world
        .response_json
        .as_ref()
        .and_then(|j| j.pointer("/item/id"))
        .and_then(|v| v.as_str())
        .expect("Could not get command UUID from GET response")
        .to_string();

    // Now cancel using the UUID
    let body = json!({});
    world
        .post_json(
            &format!("/api/v1/commands/{}/{}/cancel", deployment_hash, uuid),
            &body,
        )
        .await;
}

// ─── Snapshot steps ──────────────────────────────────────────────

#[when(regex = r#"^I get the snapshot for deployment "(.+)"$"#)]
async fn get_snapshot(world: &mut StepWorld, deployment_hash: String) {
    world
        .get(&format!("/api/v1/agent/deployments/{}", deployment_hash))
        .await;
}

#[when("I get the project snapshot for the stored project")]
async fn get_project_snapshot(world: &mut StepWorld) {
    let project_id = world
        .stored_ids
        .get("deployment_project_id")
        .expect("No stored deployment_project_id")
        .clone();
    world
        .get(&format!("/api/v1/agent/project/{}", project_id))
        .await;
}

// ─── Validation / edge-case command steps ────────────────────────

#[when("I create a command with empty deployment hash")]
async fn create_command_empty_hash(world: &mut StepWorld) {
    let body = json!({
        "deployment_hash": "",
        "command_type": "health",
        "parameters": {"app_code": "nginx"},
    });
    world.post_json("/api/v1/commands", &body).await;
}

#[when("I create a command with empty command type")]
async fn create_command_empty_type(world: &mut StepWorld) {
    let body = json!({
        "deployment_hash": "bdd-cmd-deploy",
        "command_type": "",
    });
    world.post_json("/api/v1/commands", &body).await;
}

#[when("I create a logs command with invalid limit")]
async fn create_logs_invalid_limit(world: &mut StepWorld) {
    let body = json!({
        "deployment_hash": "bdd-cmd-deploy",
        "command_type": "logs",
        "parameters": {
            "app_code": "nginx",
            "limit": 9999,
            "streams": ["stdout"]
        },
    });
    world.post_json("/api/v1/commands", &body).await;
}

#[when(regex = r#"^I get command "([^"]+)" for deployment "([^"]+)"$"#)]
async fn get_command_by_id(world: &mut StepWorld, cmd_id: String, deployment_hash: String) {
    world
        .get(&format!("/api/v1/commands/{}/{}", deployment_hash, cmd_id))
        .await;
}

#[when(regex = r#"^I create a health-all command for deployment "([^"]+)"$"#)]
async fn create_health_all(world: &mut StepWorld, deployment_hash: String) {
    let body = json!({
        "deployment_hash": deployment_hash,
        "command_type": "health",
        "parameters": {"app_code": "all"},
    });
    world.post_json("/api/v1/commands", &body).await;
    store_command_id(world);
}

#[when(regex = r#"^I create a restart command for app "([^"]+)" on deployment "([^"]+)"$"#)]
async fn create_restart_command(world: &mut StepWorld, app_code: String, deployment_hash: String) {
    let body = json!({
        "deployment_hash": deployment_hash,
        "command_type": "restart",
        "parameters": {"app_code": app_code},
    });
    world.post_json("/api/v1/commands", &body).await;
    store_command_id(world);
}

// ─── Enqueue steps ───────────────────────────────────────────────

#[when(regex = r#"^I enqueue a command for deployment "([^"]+)" with type "([^"]+)"$"#)]
async fn enqueue_command(world: &mut StepWorld, deployment_hash: String, cmd_type: String) {
    let body = json!({
        "deployment_hash": deployment_hash,
        "command_type": cmd_type,
        "parameters": {"app_code": "nginx"},
        "timeout_seconds": 30,
    });
    world
        .post_json("/api/v1/agent/commands/enqueue", &body)
        .await;
    store_command_id(world);
}

// ─── Audit steps ─────────────────────────────────────────────────

async fn do_ingest(
    world: &mut StepWorld,
    installation_hash: &str,
    key: &str,
    events: serde_json::Value,
) {
    let url = format!("{}/api/v1/agent/audit", world.base_url);
    let body = json!({
        "installation_hash": installation_hash,
        "events": events,
    });
    let resp = world
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", world.auth_token))
        .header("x-internal-key", key)
        .json(&body)
        .send()
        .await
        .expect("Audit ingest request failed");
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    world.status_code = Some(status);
    world.response_body = Some(text.clone());
    world.response_json = serde_json::from_str(&text).ok();
}

#[when(regex = r#"^I ingest audit events for installation "([^"]+)"$"#)]
async fn ingest_audit(world: &mut StepWorld, installation_hash: String) {
    let events = json!([
        {"id": 1, "event_type": "deploy_start", "payload": {"key": "v1"}, "created_at": 1711000000},
        {"id": 2, "event_type": "deploy_end", "payload": {"key": "v2"}, "created_at": 1711000100}
    ]);
    do_ingest(world, &installation_hash, "bdd-internal-key", events).await;
}

#[when("I ingest audit events with invalid internal key")]
async fn ingest_audit_invalid_key(world: &mut StepWorld) {
    let events = json!([
        {"id": 1, "event_type": "test", "payload": {}, "created_at": 1711000000}
    ]);
    do_ingest(world, "whatever", "wrong-key", events).await;
}

#[given(regex = r#"^I have ingested audit events for installation "([^"]+)"$"#)]
async fn given_ingested_audit(world: &mut StepWorld, installation_hash: String) {
    let events = json!([
        {"id": 1, "event_type": "deploy_start", "payload": {"status": "ok"}, "created_at": 1711000000},
        {"id": 2, "event_type": "deploy_end", "payload": {"status": "done"}, "created_at": 1711000100}
    ]);
    do_ingest(world, &installation_hash, "bdd-internal-key", events).await;
    assert_eq!(
        world.status_code.unwrap_or(0),
        200,
        "Audit ingest setup failed: {}",
        world.response_body.as_deref().unwrap_or("")
    );
}

#[when(regex = r#"^I query audit events for installation "([^"]+)"$"#)]
async fn query_audit(world: &mut StepWorld, installation_hash: String) {
    world
        .get(&format!(
            "/api/v1/agent/audit?installation_hash={}",
            installation_hash
        ))
        .await;
}

#[when(regex = r#"^I query audit events for installation "([^"]+)" with event type "([^"]+)"$"#)]
async fn query_audit_filtered(
    world: &mut StepWorld,
    installation_hash: String,
    event_type: String,
) {
    world
        .get(&format!(
            "/api/v1/agent/audit?installation_hash={}&event_type={}",
            installation_hash, event_type
        ))
        .await;
}

#[when(regex = r#"^I ingest empty audit batch for installation "([^"]+)"$"#)]
async fn ingest_empty_audit(world: &mut StepWorld, installation_hash: String) {
    do_ingest(world, &installation_hash, "bdd-internal-key", json!([])).await;
}
