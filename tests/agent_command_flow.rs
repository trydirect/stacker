mod common;

use chrono::Utc;
use serde_json::json;
use stacker::db;
use stacker::models::{Command, CommandPriority};
use std::time::Duration;

fn fixture(path: &str) -> serde_json::Value {
    let body = match path {
        "activate_pipe.success.report.json" => {
            include_str!("fixtures/pipe-contract/activate_pipe.success.report.json")
        }
        "deactivate_pipe.success.report.json" => {
            include_str!("fixtures/pipe-contract/deactivate_pipe.success.report.json")
        }
        other => panic!("unknown fixture: {}", other),
    };

    serde_json::from_str(body).expect("fixture should be valid json")
}

async fn create_test_deployment(app: &common::TestApp, project_name: &str, deployment_hash: &str) {
    sqlx::query(
        "INSERT INTO project (stack_id, name, user_id, metadata, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(project_name)
    .bind("test_user_id")
    .bind(serde_json::json!({}))
    .execute(&app.db_pool)
    .await
    .expect("Failed to create project");

    let project_id: i32 = sqlx::query_scalar("SELECT id FROM project WHERE name = $1 LIMIT 1")
        .bind(project_name)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to get project ID");

    sqlx::query(
        "INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
    )
    .bind(project_id)
    .bind(deployment_hash)
    .bind(Some("test_user_id"))
    .bind(serde_json::json!({}))
    .bind("pending")
    .execute(&app.db_pool)
    .await
    .expect("Failed to create deployment");
}

async fn register_test_agent(
    client: &reqwest::Client,
    app: &common::TestApp,
    deployment_hash: &str,
) -> (String, String) {
    let register_payload = json!({
        "deployment_hash": deployment_hash,
        "agent_version": "1.0.0",
        "capabilities": ["docker", "compose", "pipes"],
        "system_info": {
            "os": "linux",
            "arch": "x86_64",
            "memory_gb": 8
        }
    });

    let register_response = client
        .post(&format!("{}/api/v1/agent/register", &app.address))
        .json(&register_payload)
        .send()
        .await
        .expect("Failed to register agent");

    let register_result: serde_json::Value = register_response
        .json()
        .await
        .expect("Failed to parse register response");

    let agent_id = register_result["data"]["item"]["agent_id"]
        .as_str()
        .expect("Missing agent_id")
        .to_string();
    let agent_token = register_result["data"]["item"]["agent_token"]
        .as_str()
        .expect("Missing agent_token")
        .to_string();

    (agent_id, agent_token)
}

async fn create_pipe_instance(
    app: &common::TestApp,
    deployment_hash: &str,
    created_by: &str,
) -> uuid::Uuid {
    let pipe_instance_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO pipe_instances (
            id, template_id, deployment_hash, source_container, target_container, target_url,
            field_mapping_override, config_override, status, last_triggered_at, trigger_count,
            error_count, created_by, created_at, updated_at
        ) VALUES (
            $1, NULL, $2, $3, NULL, NULL, NULL, NULL, $4, NULL, 0, 0, $5, NOW(), NOW()
        )",
    )
    .bind(pipe_instance_id)
    .bind(deployment_hash)
    .bind("source-service")
    .bind("active")
    .bind(created_by)
    .execute(&app.db_pool)
    .await
    .expect("Failed to create pipe instance");

    pipe_instance_id
}

async fn queue_trigger_pipe_command(
    app: &common::TestApp,
    deployment_hash: &str,
    pipe_instance_id: uuid::Uuid,
    input_data: serde_json::Value,
) -> String {
    let command_id = format!("cmd_{}", uuid::Uuid::new_v4());
    let command = Command::new(
        command_id.clone(),
        deployment_hash.to_string(),
        "trigger_pipe".to_string(),
        "test_user_id".to_string(),
    )
    .with_priority(CommandPriority::Normal)
    .with_parameters(json!({
        "pipe_instance_id": pipe_instance_id.to_string(),
        "input_data": input_data
    }));

    let saved_command = db::command::insert(&app.db_pool, &command)
        .await
        .expect("Failed to save trigger_pipe command");

    db::command::add_to_queue(
        &app.db_pool,
        &saved_command.command_id,
        &saved_command.deployment_hash,
        &CommandPriority::Normal,
    )
    .await
    .expect("Failed to queue trigger_pipe command");

    command_id
}

async fn queue_pipe_command(
    app: &common::TestApp,
    deployment_hash: &str,
    command_type: &str,
    parameters: serde_json::Value,
) -> String {
    let command_id = format!("cmd_{}", uuid::Uuid::new_v4());
    let command = Command::new(
        command_id.clone(),
        deployment_hash.to_string(),
        command_type.to_string(),
        "test_user_id".to_string(),
    )
    .with_priority(CommandPriority::Normal)
    .with_parameters(parameters);

    let saved_command = db::command::insert(&app.db_pool, &command)
        .await
        .expect("Failed to save pipe command");

    db::command::add_to_queue(
        &app.db_pool,
        &saved_command.command_id,
        &saved_command.deployment_hash,
        &CommandPriority::Normal,
    )
    .await
    .expect("Failed to queue pipe command");

    command_id
}

async fn wait_for_command(
    client: &reqwest::Client,
    app: &common::TestApp,
    deployment_hash: &str,
    agent_id: &str,
    agent_token: &str,
) -> serde_json::Value {
    client
        .get(&format!(
            "{}/api/v1/agent/commands/wait/{}",
            &app.address, deployment_hash
        ))
        .header("X-Agent-Id", agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .timeout(Duration::from_secs(35))
        .send()
        .await
        .expect("Failed to poll for commands")
        .json()
        .await
        .expect("Failed to parse wait response")
}

/// Test the complete agent/command flow:
/// 1. Create a deployment
/// 2. Register an agent for that deployment
/// 3. Create a command for the deployment
/// 4. Agent polls and receives the command
/// 5. Agent reports command completion
#[tokio::test]
async fn test_agent_command_flow() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    // Step 1: Create a test deployment (simulating what deploy endpoint does)
    // For this test, we'll use a mock deployment_hash
    let deployment_hash = format!("test_deployment_{}", uuid::Uuid::new_v4());

    println!(
        "Testing agent/command flow with deployment_hash: {}",
        deployment_hash
    );

    // Create deployment in database (required by foreign key constraint)
    // First create a minimal project (required by deployment FK)
    sqlx::query(
        "INSERT INTO project (stack_id, name, user_id, metadata, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind("test_project_main")
    .bind("test_user_id")
    .bind(serde_json::json!({}))
    .execute(&app.db_pool)
    .await
    .expect("Failed to create project");

    let project_id: i32 =
        sqlx::query_scalar("SELECT id FROM project WHERE name = 'test_project_main' LIMIT 1")
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to get project ID");

    sqlx::query(
        "INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW())"
    )
    .bind(project_id)
    .bind(&deployment_hash)
    .bind(Some("test_user_id"))
    .bind(serde_json::json!({}))
    .bind("pending")
    .execute(&app.db_pool)
    .await
    .expect("Failed to create deployment");

    // Step 2: Register an agent
    println!("\n=== Step 2: Register Agent ===");
    let register_payload = json!({
        "deployment_hash": deployment_hash,
        "agent_version": "1.0.0",
        "capabilities": ["docker", "compose", "logs"],
        "system_info": {
            "os": "linux",
            "arch": "x86_64",
            "memory_gb": 8
        }
    });

    let register_response = client
        .post(&format!("{}/api/v1/agent/register", &app.address))
        .json(&register_payload)
        .send()
        .await
        .expect("Failed to register agent");

    println!("Register response status: {}", register_response.status());

    if !register_response.status().is_success() {
        let error_text = register_response.text().await.unwrap_or_default();
        println!("Register error: {}", error_text);
        panic!("Agent registration failed");
    }

    let register_result: serde_json::Value = register_response
        .json()
        .await
        .expect("Failed to parse register response");

    println!(
        "Register result: {}",
        serde_json::to_string_pretty(&register_result).unwrap()
    );

    let agent_id = register_result["data"]["item"]["agent_id"]
        .as_str()
        .expect("Missing agent_id")
        .to_string();
    let agent_token = register_result["data"]["item"]["agent_token"]
        .as_str()
        .expect("Missing agent_token")
        .to_string();

    println!("Agent registered: {} with token", agent_id);

    // Step 3: Create a command for this deployment
    println!("\n=== Step 3: Create Command (Authenticated) ===");
    let command_payload = json!({
        "deployment_hash": deployment_hash,
            "command_type": "restart_service",
        "priority": "high",
        "parameters": {
            "service": "web",
            "graceful": true
        },
        "timeout_seconds": 300
    });

    // Use a test Bearer token - the mock auth server will validate any token
    let create_command_response = client
        .post(&format!("{}/api/v1/commands", &app.address))
        .header("Authorization", "Bearer test_token_12345")
        .json(&command_payload)
        .send()
        .await
        .expect("Failed to create command");

    println!(
        "Create command response status: {}",
        create_command_response.status()
    );

    let status = create_command_response.status();
    if !status.is_success() {
        let error_text = create_command_response.text().await.unwrap_or_default();
        println!("Create command error: {}", error_text);
        panic!(
            "Command creation failed with status {}: {}",
            status, error_text
        );
    }

    let command_result: serde_json::Value = create_command_response
        .json()
        .await
        .expect("Failed to parse command response");

    println!(
        "Command created: {}",
        serde_json::to_string_pretty(&command_result).unwrap()
    );

    let command_id = command_result["item"]["command_id"]
        .as_str()
        .expect("Missing command_id")
        .to_string();

    // Step 4: Agent polls for commands (long-polling)
    println!("\n=== Step 4: Agent Polls for Commands ===");

    // Agent should authenticate with X-Agent-Id header and Bearer token
    let wait_response = client
        .get(&format!(
            "{}/api/v1/agent/commands/wait/{}",
            &app.address, deployment_hash
        ))
        .header("X-Agent-Id", &agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .timeout(Duration::from_secs(35)) // Longer than server's 30s timeout
        .send()
        .await
        .expect("Failed to poll for commands");

    println!("Wait response status: {}", wait_response.status());

    if !wait_response.status().is_success() {
        let error_text = wait_response.text().await.unwrap_or_default();
        println!("Wait error: {}", error_text);
        panic!("Agent wait failed");
    }

    let wait_result: serde_json::Value = wait_response
        .json()
        .await
        .expect("Failed to parse wait response");

    println!(
        "Agent received command: {}",
        serde_json::to_string_pretty(&wait_result).unwrap()
    );

    // Verify we received the command
    let received_command_id = wait_result["item"]["command_id"]
        .as_str()
        .expect("No command received");

    assert_eq!(received_command_id, command_id, "Received wrong command");

    // Step 5: Agent reports command completion
    println!("\n=== Step 5: Agent Reports Command Result ===");

    let report_payload = json!({
        "command_id": command_id,
        "deployment_hash": deployment_hash,
        "status": "completed",
        "executed_by": "compose_agent",
        "started_at": Utc::now(),
        "completed_at": Utc::now(),
        "result": {
            "service_restarted": true,
            "restart_time_seconds": 5.2,
            "final_status": "running"
        },
        "metadata": {
            "execution_node": "worker-1"
        }
    });

    let report_response = client
        .post(&format!("{}/api/v1/agent/commands/report", &app.address))
        .header("X-Agent-Id", &agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .json(&report_payload)
        .send()
        .await
        .expect("Failed to report command");

    println!("Report response status: {}", report_response.status());

    if !report_response.status().is_success() {
        let error_text = report_response.text().await.unwrap_or_default();
        println!("Report error: {}", error_text);
        panic!("Command report failed");
    }

    let report_result: serde_json::Value = report_response
        .json()
        .await
        .expect("Failed to parse report response");

    println!(
        "Report result: {}",
        serde_json::to_string_pretty(&report_result).unwrap()
    );

    let stored_metadata: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT metadata FROM commands WHERE command_id = $1")
            .bind(&command_id)
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to fetch command metadata");

    assert_eq!(
        stored_metadata
            .as_ref()
            .and_then(|value| value.get("executed_by"))
            .and_then(|value| value.as_str()),
        Some("compose_agent")
    );

    println!("\n=== Test Completed Successfully ===");
}

#[tokio::test]
async fn test_trigger_pipe_report_persists_execution_history() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let deployment_hash = format!("test_pipe_deployment_{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO project (stack_id, name, user_id, metadata, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind("test_pipe_project")
    .bind("test_user_id")
    .bind(serde_json::json!({}))
    .execute(&app.db_pool)
    .await
    .expect("Failed to create project");

    let project_id: i32 =
        sqlx::query_scalar("SELECT id FROM project WHERE name = 'test_pipe_project' LIMIT 1")
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to get project ID");

    sqlx::query(
        "INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW())"
    )
    .bind(project_id)
    .bind(&deployment_hash)
    .bind(Some("test_user_id"))
    .bind(serde_json::json!({}))
    .bind("pending")
    .execute(&app.db_pool)
    .await
    .expect("Failed to create deployment");

    let pipe_instance_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO pipe_instances (
            id, template_id, deployment_hash, source_container, target_container, target_url,
            field_mapping_override, config_override, status, last_triggered_at, trigger_count,
            error_count, created_by, created_at, updated_at
        ) VALUES (
            $1, NULL, $2, $3, NULL, NULL, NULL, NULL, $4, NULL, 0, 0, $5, NOW(), NOW()
        )",
    )
    .bind(pipe_instance_id)
    .bind(&deployment_hash)
    .bind("source-service")
    .bind("active")
    .bind("test_user_id")
    .execute(&app.db_pool)
    .await
    .expect("Failed to create pipe instance");

    let register_payload = json!({
        "deployment_hash": deployment_hash,
        "agent_version": "1.0.0",
        "capabilities": ["docker", "compose", "pipes"],
        "system_info": {
            "os": "linux",
            "arch": "x86_64",
            "memory_gb": 8
        }
    });

    let register_response = client
        .post(&format!("{}/api/v1/agent/register", &app.address))
        .json(&register_payload)
        .send()
        .await
        .expect("Failed to register agent");

    let register_result: serde_json::Value = register_response
        .json()
        .await
        .expect("Failed to parse register response");

    let agent_id = register_result["data"]["item"]["agent_id"]
        .as_str()
        .expect("Missing agent_id")
        .to_string();
    let agent_token = register_result["data"]["item"]["agent_token"]
        .as_str()
        .expect("Missing agent_token")
        .to_string();

    let command_id = format!("cmd_{}", uuid::Uuid::new_v4());
    let command = Command::new(
        command_id.clone(),
        deployment_hash.clone(),
        "trigger_pipe".to_string(),
        "test_user_id".to_string(),
    )
    .with_priority(CommandPriority::Normal)
    .with_parameters(json!({
        "pipe_instance_id": pipe_instance_id.to_string(),
        "input_data": {
            "invoice_id": "inv-1"
        }
    }));

    let saved_command = db::command::insert(&app.db_pool, &command)
        .await
        .expect("Failed to save trigger_pipe command");
    db::command::add_to_queue(
        &app.db_pool,
        &saved_command.command_id,
        &saved_command.deployment_hash,
        &CommandPriority::Normal,
    )
    .await
    .expect("Failed to queue trigger_pipe command");

    let wait_response = client
        .get(&format!(
            "{}/api/v1/agent/commands/wait/{}",
            &app.address, deployment_hash
        ))
        .header("X-Agent-Id", &agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .timeout(Duration::from_secs(35))
        .send()
        .await
        .expect("Failed to poll for commands");

    let wait_result: serde_json::Value = wait_response
        .json()
        .await
        .expect("Failed to parse wait response");

    assert_eq!(
        wait_result["item"]["command_id"].as_str(),
        Some(command_id.as_str())
    );
    assert_eq!(wait_result["item"]["type"].as_str(), Some("trigger_pipe"));
    assert_eq!(
        wait_result["item"]["parameters"]["pipe_instance_id"].as_str(),
        Some(pipe_instance_id.to_string().as_str())
    );

    let report_payload = json!({
        "command_id": command_id,
        "deployment_hash": deployment_hash,
        "status": "completed",
        "executed_by": "compose_agent",
        "started_at": Utc::now(),
        "completed_at": Utc::now(),
        "result": {
            "type": "trigger_pipe",
            "deployment_hash": deployment_hash,
            "pipe_instance_id": pipe_instance_id.to_string(),
            "success": true,
            "source_data": { "invoice_id": "inv-1" },
            "mapped_data": { "customer_id": "cust-1" },
            "target_response": { "queued": true },
            "triggered_at": Utc::now(),
            "trigger_type": "manual"
        }
    });

    let report_response = client
        .post(&format!("{}/api/v1/agent/commands/report", &app.address))
        .header("X-Agent-Id", &agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .json(&report_payload)
        .send()
        .await
        .expect("Failed to report trigger_pipe command");

    assert!(
        report_response.status().is_success(),
        "trigger_pipe report failed: {}",
        report_response.text().await.unwrap_or_default()
    );

    let stored_metadata: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT metadata FROM commands WHERE command_id = $1")
            .bind(&command_id)
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to fetch command metadata");

    assert_eq!(
        stored_metadata
            .as_ref()
            .and_then(|value| value.get("executed_by"))
            .and_then(|value| value.as_str()),
        Some("compose_agent")
    );

    let execution_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pipe_executions WHERE pipe_instance_id = $1")
            .bind(pipe_instance_id)
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to count pipe executions");
    assert_eq!(execution_count, 1);

    let execution_row: (
        String,
        String,
        String,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
    ) = sqlx::query_as(
        "SELECT status, trigger_type, created_by, source_data, mapped_data, target_response
             FROM pipe_executions
             WHERE pipe_instance_id = $1
             ORDER BY started_at DESC
             LIMIT 1",
    )
    .bind(pipe_instance_id)
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch pipe execution row");

    assert_eq!(execution_row.0, "success");
    assert_eq!(execution_row.1, "manual");
    assert_eq!(execution_row.2, "compose_agent");
    assert_eq!(execution_row.3, json!({ "invoice_id": "inv-1" }));
    assert_eq!(execution_row.4, json!({ "customer_id": "cust-1" }));
    assert_eq!(execution_row.5, json!({ "queued": true }));

    let instance_row: (i64, i64, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
        "SELECT trigger_count, error_count, last_triggered_at FROM pipe_instances WHERE id = $1",
    )
    .bind(pipe_instance_id)
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch pipe instance counters");

    assert_eq!(instance_row.0, 1);
    assert_eq!(instance_row.1, 0);
    assert!(instance_row.2.is_some());
}

/// Test agent heartbeat mechanism
#[tokio::test]
async fn test_agent_heartbeat() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let deployment_hash = format!("test_hb_{}", uuid::Uuid::new_v4());

    // First, create a deployment in the database (required by foreign key)
    // Create a minimal project first (required by deployment FK)
    sqlx::query(
        "INSERT INTO project (stack_id, name, user_id, metadata, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind("test_project")
    .bind("test_user_id")
    .bind(serde_json::json!({}))
    .execute(&app.db_pool)
    .await
    .expect("Failed to create project");

    // Get the project ID we just created
    let project_id: i32 =
        sqlx::query_scalar("SELECT id FROM project WHERE name = 'test_project' LIMIT 1")
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to get project ID");

    // Create deployment
    sqlx::query(
        "INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW())"
    )
    .bind(project_id)
    .bind(&deployment_hash)
    .bind(Some("test_user_id"))
    .bind(serde_json::json!({}))
    .bind("pending")
    .execute(&app.db_pool)
    .await
    .expect("Failed to create deployment");

    // Register agent
    let register_payload = json!({
        "deployment_hash": deployment_hash,
        "agent_version": "1.0.0",
        "capabilities": ["docker"],
        "system_info": {"os": "linux"}
    });

    let register_response = client
        .post(&format!("{}/api/v1/agent/register", &app.address))
        .json(&register_payload)
        .send()
        .await
        .expect("Failed to register");

    let status = register_response.status();

    if !status.is_success() {
        let body_text = register_response.text().await.unwrap_or_default();
        panic!(
            "Registration failed. Status: {}, Body: {}",
            status, body_text
        );
    }

    let register_result: serde_json::Value = register_response.json().await.unwrap();
    let agent_id = register_result["data"]["item"]["agent_id"]
        .as_str()
        .unwrap();
    let agent_token = register_result["data"]["item"]["agent_token"]
        .as_str()
        .unwrap();

    // Poll for commands (this updates heartbeat)
    let wait_response = client
        .get(&format!(
            "{}/api/v1/agent/commands/wait/{}",
            &app.address, deployment_hash
        ))
        .header("X-Agent-Id", agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .timeout(Duration::from_secs(35))
        .send()
        .await
        .expect("Failed to poll");

    // Should succeed even if no commands (updates heartbeat and returns empty)
    println!("Heartbeat/wait status: {}", wait_response.status());

    // Either 200 with no command or 204 is acceptable
    assert!(
        wait_response.status().is_success(),
        "Wait request should succeed for heartbeat"
    );

    println!("Heartbeat test completed");
}

/// Test command priority ordering
#[tokio::test]
#[ignore] // Requires auth setup
async fn test_command_priority_ordering() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let deployment_hash = format!("test_priority_{}", uuid::Uuid::new_v4());

    // Register agent
    let register_payload = json!({
        "deployment_hash": deployment_hash,
        "agent_version": "1.0.0",
        "capabilities": ["docker"],
        "system_info": {"os": "linux"}
    });

    let register_response = client
        .post(&format!("{}/api/v1/agent/register", &app.address))
        .json(&register_payload)
        .send()
        .await
        .expect("Failed to register");

    let register_result: serde_json::Value = register_response.json().await.unwrap();
    let agent_id = register_result["data"]["item"]["agent_id"]
        .as_str()
        .unwrap();
    let agent_token = register_result["data"]["item"]["agent_token"]
        .as_str()
        .unwrap();

    // Create commands with different priorities (requires auth - will fail without it)
    for (priority, cmd_type) in &[
        ("low", "backup"),
        ("critical", "restart"),
        ("normal", "logs"),
    ] {
        let cmd_payload = json!({
            "deployment_hash": deployment_hash,
                "command_type": cmd_type,
            "priority": priority,
            "parameters": {}
        });

        client
            .post(&format!("{}/api/v1/commands", &app.address))
            .json(&cmd_payload)
            .send()
            .await
            .expect("Failed to create command");
    }

    // Agent should receive critical command first
    let wait_response = client
        .get(&format!(
            "{}/api/v1/agent/commands/wait/{}",
            &app.address, deployment_hash
        ))
        .header("X-Agent-Id", agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .send()
        .await
        .expect("Failed to poll");

    let wait_result: serde_json::Value = wait_response.json().await.unwrap();
    let received_type = wait_result["item"]["type"].as_str().unwrap();

    assert_eq!(
        received_type, "restart",
        "Should receive critical priority command first"
    );
}

/// Test authenticated command creation
#[tokio::test]
async fn test_authenticated_command_creation() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let deployment_hash = format!("test_cmd_{}", uuid::Uuid::new_v4());

    // Create project and deployment
    sqlx::query(
        "INSERT INTO project (stack_id, name, user_id, metadata, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind("test_project_cmd")
    .bind("test_user_id")
    .bind(serde_json::json!({}))
    .execute(&app.db_pool)
    .await
    .expect("Failed to create project");

    let project_id: i32 =
        sqlx::query_scalar("SELECT id FROM project WHERE name = 'test_project_cmd' LIMIT 1")
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to get project ID");

    sqlx::query(
        "INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW())"
    )
    .bind(project_id)
    .bind(&deployment_hash)
    .bind(Some("test_user_id"))
    .bind(serde_json::json!({}))
    .bind("pending")
    .execute(&app.db_pool)
    .await
    .expect("Failed to create deployment");

    println!("\n=== Test 1: Command creation without authentication (should fail) ===");
    let cmd_payload = json!({
        "deployment_hash": deployment_hash,
            "command_type": "deploy",
        "priority": "normal",
        "parameters": {}
    });

    let response_no_auth = client
        .post(&format!("{}/api/v1/commands", &app.address))
        .json(&cmd_payload)
        .send()
        .await
        .expect("Failed to send request");

    println!("No auth response status: {}", response_no_auth.status());
    assert_eq!(
        response_no_auth.status(),
        403,
        "Should return 403 without authentication"
    );

    println!("\n=== Test 2: Command creation with authentication (should succeed) ===");
    let response_with_auth = client
        .post(&format!("{}/api/v1/commands", &app.address))
        .header("Authorization", "Bearer test_token_authenticated")
        .json(&cmd_payload)
        .send()
        .await
        .expect("Failed to send authenticated request");

    let status = response_with_auth.status();
    println!("With auth response status: {}", status);

    if !status.is_success() {
        let error_body = response_with_auth.text().await.unwrap_or_default();
        println!("Error body: {}", error_body);
        panic!("Authenticated command creation failed: {}", error_body);
    }

    let result: serde_json::Value = response_with_auth.json().await.unwrap();
    println!(
        "Created command: {}",
        serde_json::to_string_pretty(&result).unwrap()
    );

    // Verify command was created
    let command_id = result["item"]["command_id"]
        .as_str()
        .expect("Missing command_id");
    assert!(!command_id.is_empty(), "Command ID should not be empty");

    println!("\n=== Test 3: List commands for deployment ===");
    let list_response = client
        .get(&format!(
            "{}/api/v1/commands/{}",
            &app.address, deployment_hash
        ))
        .header("Authorization", "Bearer test_token_authenticated")
        .send()
        .await
        .expect("Failed to list commands");

    assert!(
        list_response.status().is_success(),
        "Should list commands successfully"
    );
    let list_result: serde_json::Value = list_response.json().await.unwrap();
    println!(
        "Commands list: {}",
        serde_json::to_string_pretty(&list_result).unwrap()
    );

    println!("\n=== Authenticated Command Creation Test Completed ===");
}

/// Test command priorities and user permissions
#[tokio::test]
async fn test_command_priorities_and_permissions() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let deployment_hash = format!("test_prio_{}", uuid::Uuid::new_v4());

    // Create project and deployment
    sqlx::query(
        "INSERT INTO project (stack_id, name, user_id, metadata, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind("test_project_prio")
    .bind("test_user_id")
    .bind(serde_json::json!({}))
    .execute(&app.db_pool)
    .await
    .expect("Failed to create project");

    let project_id: i32 =
        sqlx::query_scalar("SELECT id FROM project WHERE name = 'test_project_prio' LIMIT 1")
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to get project ID");

    sqlx::query(
        "INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW())"
    )
    .bind(project_id)
    .bind(&deployment_hash)
    .bind(Some("test_user_id"))
    .bind(serde_json::json!({}))
    .bind("pending")
    .execute(&app.db_pool)
    .await
    .expect("Failed to create deployment");

    // Create commands with different priorities
    let priorities = vec![
        ("low", "backup"),
        ("critical", "emergency_restart"),
        ("normal", "update_config"),
        ("high", "restart_service"),
    ];

    println!("\n=== Creating commands with different priorities ===");
    for (priority, cmd_type) in &priorities {
        let payload = json!({
            "deployment_hash": deployment_hash,
                "command_type": cmd_type,
            "priority": priority,
            "parameters": {}
        });

        let response = client
            .post(&format!("{}/api/v1/commands", &app.address))
            .header("Authorization", "Bearer test_token")
            .json(&payload)
            .send()
            .await
            .expect("Failed to create command");

        println!(
            "Created {} priority command '{}': {}",
            priority,
            cmd_type,
            response.status()
        );
        assert!(
            response.status().is_success(),
            "Should create {} priority command",
            priority
        );
    }

    // Register agent to poll for commands
    let register_payload = json!({
        "deployment_hash": deployment_hash,
        "agent_version": "1.0.0",
        "capabilities": ["docker"],
        "system_info": {"os": "linux"}
    });

    let register_response = client
        .post(&format!("{}/api/v1/agent/register", &app.address))
        .json(&register_payload)
        .send()
        .await
        .expect("Failed to register agent");

    let register_result: serde_json::Value = register_response.json().await.unwrap();
    let agent_id = register_result["data"]["item"]["agent_id"]
        .as_str()
        .unwrap();
    let agent_token = register_result["data"]["item"]["agent_token"]
        .as_str()
        .unwrap();

    // Agent polls - should receive critical priority first
    println!("\n=== Agent polling for commands (should receive critical first) ===");
    let wait_response = client
        .get(&format!(
            "{}/api/v1/agent/commands/wait/{}",
            &app.address, deployment_hash
        ))
        .header("X-Agent-Id", agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .expect("Failed to poll");

    if wait_response.status().is_success() {
        let wait_result: serde_json::Value = wait_response.json().await.unwrap();
        if let Some(cmd_type) = wait_result["item"]["type"].as_str() {
            println!("Received command type: {}", cmd_type);
            assert_eq!(
                cmd_type, "emergency_restart",
                "Should receive critical priority command first"
            );
        } else {
            println!("No command in response (queue might be empty)");
        }
    } else {
        println!(
            "Wait returned non-success status: {} (might be expected if no commands)",
            wait_response.status()
        );
    }

    println!("\n=== Command Priority Test Completed ===");
}

#[tokio::test]
async fn test_trigger_pipe_failed_report_persists_execution_history() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();
    let deployment_hash = format!("test_pipe_fail_deployment_{}", uuid::Uuid::new_v4());

    create_test_deployment(&app, "test_pipe_fail_project", &deployment_hash).await;
    let pipe_instance_id = create_pipe_instance(&app, &deployment_hash, "test_user_id").await;
    let (agent_id, agent_token) = register_test_agent(&client, &app, &deployment_hash).await;
    let command_id = queue_trigger_pipe_command(
        &app,
        &deployment_hash,
        pipe_instance_id,
        json!({"invoice_id": "inv-fail"}),
    )
    .await;

    let wait_result =
        wait_for_command(&client, &app, &deployment_hash, &agent_id, &agent_token).await;
    assert_eq!(
        wait_result["item"]["command_id"].as_str(),
        Some(command_id.as_str())
    );
    assert_eq!(wait_result["item"]["type"].as_str(), Some("trigger_pipe"));

    let report_payload = json!({
        "command_id": command_id,
        "deployment_hash": deployment_hash,
        "status": "failed",
        "executed_by": "compose_agent",
        "started_at": Utc::now(),
        "completed_at": Utc::now(),
        "result": {
            "type": "trigger_pipe",
            "deployment_hash": deployment_hash,
            "pipe_instance_id": pipe_instance_id.to_string(),
            "success": false,
            "source_data": { "invoice_id": "inv-fail" },
            "mapped_data": { "customer_id": "cust-fail" },
            "target_response": { "queued": false },
            "error": "target endpoint returned 500",
            "triggered_at": Utc::now(),
            "trigger_type": "manual"
        }
    });

    let report_response = client
        .post(&format!("{}/api/v1/agent/commands/report", &app.address))
        .header("X-Agent-Id", &agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .json(&report_payload)
        .send()
        .await
        .expect("Failed to report failed trigger_pipe command");

    assert!(
        report_response.status().is_success(),
        "failed trigger_pipe report failed: {}",
        report_response.text().await.unwrap_or_default()
    );

    let execution_row: (i64, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(status), MAX(error) FROM pipe_executions WHERE pipe_instance_id = $1",
    )
    .bind(pipe_instance_id)
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch failed pipe execution");

    assert_eq!(execution_row.0, 1);
    assert_eq!(execution_row.1.as_deref(), Some("failed"));
    assert_eq!(
        execution_row.2.as_deref(),
        Some("target endpoint returned 500")
    );

    let instance_row: (i64, i64) =
        sqlx::query_as("SELECT trigger_count, error_count FROM pipe_instances WHERE id = $1")
            .bind(pipe_instance_id)
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to fetch pipe instance counters");

    assert_eq!(instance_row.0, 1);
    assert_eq!(instance_row.1, 1);
}

#[tokio::test]
async fn test_replay_trigger_pipe_report_updates_existing_replay_execution() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();
    let deployment_hash = format!("test_pipe_replay_deployment_{}", uuid::Uuid::new_v4());

    create_test_deployment(&app, "test_pipe_replay_project", &deployment_hash).await;
    let pipe_instance_id = create_pipe_instance(&app, &deployment_hash, "test_user_id").await;
    let (agent_id, agent_token) = register_test_agent(&client, &app, &deployment_hash).await;

    let original_execution_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO pipe_executions (
            id, pipe_instance_id, deployment_hash, trigger_type, status, source_data,
            mapped_data, target_response, error, duration_ms, replay_of, created_by, started_at, completed_at
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, NULL, 100, NULL, $9, NOW(), NOW()
        )",
    )
    .bind(original_execution_id)
    .bind(pipe_instance_id)
    .bind(&deployment_hash)
    .bind("manual")
    .bind("success")
    .bind(json!({"invoice_id": "inv-replay"}))
    .bind(json!({"customer_id": "cust-replay"}))
    .bind(json!({"queued": true}))
    .bind("test_user_id")
    .execute(&app.db_pool)
    .await
    .expect("Failed to seed original execution");

    let replay_execution = stacker::models::pipe::PipeExecution::new(
        pipe_instance_id,
        Some(deployment_hash.clone()),
        "replay".to_string(),
        "test_user_id".to_string(),
    )
    .with_replay_of(original_execution_id);

    let replay_execution = db::pipe::insert_execution(&app.db_pool, &replay_execution)
        .await
        .expect("Failed to seed replay execution");

    let replay_execution_id = replay_execution.id.to_string();

    let replay_command_id = queue_trigger_pipe_command(
        &app,
        &deployment_hash,
        pipe_instance_id,
        json!({"invoice_id": "inv-replay"}),
    )
    .await;

    let wait_result =
        wait_for_command(&client, &app, &deployment_hash, &agent_id, &agent_token).await;
    assert_eq!(
        wait_result["item"]["command_id"].as_str(),
        Some(replay_command_id.as_str())
    );
    assert_eq!(wait_result["item"]["type"].as_str(), Some("trigger_pipe"));

    let report_payload = json!({
        "command_id": replay_command_id,
        "deployment_hash": deployment_hash,
        "status": "completed",
        "executed_by": "compose_agent",
        "started_at": Utc::now(),
        "completed_at": Utc::now(),
        "result": {
            "type": "trigger_pipe",
            "deployment_hash": deployment_hash,
            "pipe_instance_id": pipe_instance_id.to_string(),
            "success": true,
            "source_data": { "invoice_id": "inv-replay" },
            "mapped_data": { "customer_id": "cust-replay-updated" },
            "target_response": { "queued": true, "replayed": true },
            "triggered_at": Utc::now(),
            "trigger_type": "replay"
        }
    });

    let report_response = client
        .post(&format!("{}/api/v1/agent/commands/report", &app.address))
        .header("X-Agent-Id", &agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .json(&report_payload)
        .send()
        .await
        .expect("Failed to report replay trigger_pipe command");

    assert!(
        report_response.status().is_success(),
        "replay trigger_pipe report failed: {}",
        report_response.text().await.unwrap_or_default()
    );

    let replay_rows: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM pipe_executions WHERE replay_of = $1")
            .bind(original_execution_id)
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to count replay rows");
    assert_eq!(replay_rows.0, 1);

    let replay_execution: (String, Option<serde_json::Value>, Option<serde_json::Value>) =
        sqlx::query_as(
            "SELECT status, mapped_data, target_response
             FROM pipe_executions
             WHERE id = $1",
        )
        .bind(uuid::Uuid::parse_str(&replay_execution_id).expect("invalid replay execution id"))
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch replay execution");

    assert_eq!(replay_execution.0, "success");
    assert_eq!(
        replay_execution.1,
        Some(json!({"customer_id": "cust-replay-updated"}))
    );
    assert_eq!(
        replay_execution.2,
        Some(json!({"queued": true, "replayed": true}))
    );
}

#[tokio::test]
async fn test_activate_pipe_report_accepts_runtime_lifecycle_shape() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();
    let deployment_hash = format!("test_activate_pipe_deployment_{}", uuid::Uuid::new_v4());

    create_test_deployment(&app, "test_activate_pipe_project", &deployment_hash).await;
    let pipe_instance_id = create_pipe_instance(&app, &deployment_hash, "test_user_id").await;
    let (agent_id, agent_token) = register_test_agent(&client, &app, &deployment_hash).await;

    let command_id = queue_pipe_command(
        &app,
        &deployment_hash,
        "activate_pipe",
        json!({
            "pipe_instance_id": pipe_instance_id.to_string(),
            "target_url": "https://example.com",
            "trigger_type": "webhook"
        }),
    )
    .await;

    let wait_result =
        wait_for_command(&client, &app, &deployment_hash, &agent_id, &agent_token).await;
    assert_eq!(
        wait_result["item"]["command_id"].as_str(),
        Some(command_id.as_str())
    );
    assert_eq!(wait_result["item"]["type"].as_str(), Some("activate_pipe"));

    let mut report_result = fixture("activate_pipe.success.report.json");
    report_result["pipe_instance_id"] = json!(pipe_instance_id.to_string());
    report_result["deployment_hash"] = json!(deployment_hash.clone());

    let report_payload = json!({
        "command_id": command_id,
        "deployment_hash": deployment_hash,
        "status": "completed",
        "executed_by": "compose_agent",
        "started_at": Utc::now(),
        "completed_at": Utc::now(),
        "result": report_result
    });

    let report_response = client
        .post(&format!("{}/api/v1/agent/commands/report", &app.address))
        .header("X-Agent-Id", &agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .json(&report_payload)
        .send()
        .await
        .expect("Failed to report activate_pipe command");

    assert!(
        report_response.status().is_success(),
        "activate_pipe report failed: {}",
        report_response.text().await.unwrap_or_default()
    );

    let stored_result: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT result FROM commands WHERE command_id = $1")
            .bind(&command_id)
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to fetch activate_pipe stored result");

    let stored_result = stored_result.expect("activate_pipe result should be stored");
    assert_eq!(stored_result["active"], true);
    assert_eq!(stored_result["trigger_type"], "webhook");
    assert_eq!(stored_result["lifecycle"]["state"], "active");
}

#[tokio::test]
async fn test_deactivate_pipe_report_accepts_runtime_lifecycle_shape() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();
    let deployment_hash = format!("test_deactivate_pipe_deployment_{}", uuid::Uuid::new_v4());

    create_test_deployment(&app, "test_deactivate_pipe_project", &deployment_hash).await;
    let pipe_instance_id = create_pipe_instance(&app, &deployment_hash, "test_user_id").await;
    let (agent_id, agent_token) = register_test_agent(&client, &app, &deployment_hash).await;

    let command_id = queue_pipe_command(
        &app,
        &deployment_hash,
        "deactivate_pipe",
        json!({
            "pipe_instance_id": pipe_instance_id.to_string()
        }),
    )
    .await;

    let wait_result =
        wait_for_command(&client, &app, &deployment_hash, &agent_id, &agent_token).await;
    assert_eq!(
        wait_result["item"]["command_id"].as_str(),
        Some(command_id.as_str())
    );
    assert_eq!(
        wait_result["item"]["type"].as_str(),
        Some("deactivate_pipe")
    );

    let mut report_result = fixture("deactivate_pipe.success.report.json");
    report_result["pipe_instance_id"] = json!(pipe_instance_id.to_string());
    report_result["deployment_hash"] = json!(deployment_hash.clone());

    let report_payload = json!({
        "command_id": command_id,
        "deployment_hash": deployment_hash,
        "status": "completed",
        "executed_by": "compose_agent",
        "started_at": Utc::now(),
        "completed_at": Utc::now(),
        "result": report_result
    });

    let report_response = client
        .post(&format!("{}/api/v1/agent/commands/report", &app.address))
        .header("X-Agent-Id", &agent_id)
        .header("Authorization", format!("Bearer {}", agent_token))
        .json(&report_payload)
        .send()
        .await
        .expect("Failed to report deactivate_pipe command");

    assert!(
        report_response.status().is_success(),
        "deactivate_pipe report failed: {}",
        report_response.text().await.unwrap_or_default()
    );

    let stored_result: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT result FROM commands WHERE command_id = $1")
            .bind(&command_id)
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to fetch deactivate_pipe stored result");

    let stored_result = stored_result.expect("deactivate_pipe result should be stored");
    assert_eq!(stored_result["active"], false);
    assert_eq!(stored_result["removed"], true);
    assert_eq!(stored_result["lifecycle"]["state"], "inactive");
}
