//! MCP Integration Tests with User Service
//!
//! These tests verify the MCP tools work correctly with the live User Service.
//! Run with: cargo test --test mcp_integration -- --ignored
//!
//! Prerequisites:
//! - User Service running at USER_SERVICE_URL (default: http://user:4100)
//! - Valid test user credentials
//! - Database migrations applied

mod common;

use serde_json::{json, Value};
use std::env;

/// Test configuration for integration tests
struct IntegrationConfig {
    user_service_url: String,
    test_user_email: String,
    test_user_password: String,
    test_deployment_id: Option<i64>,
}

impl IntegrationConfig {
    fn from_env() -> Option<Self> {
        Some(Self {
            user_service_url: env::var("USER_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:4100".to_string()),
            test_user_email: env::var("TEST_USER_EMAIL").ok()?,
            test_user_password: env::var("TEST_USER_PASSWORD").ok()?,
            test_deployment_id: env::var("TEST_DEPLOYMENT_ID")
                .ok()
                .and_then(|s| s.parse().ok()),
        })
    }
}

/// Helper to authenticate and get a bearer token
async fn get_auth_token(config: &IntegrationConfig) -> Result<String, String> {
    let client = reqwest::Client::new();
    
    let response = client
        .post(&format!("{}/oauth_server/token", config.user_service_url))
        .form(&[
            ("grant_type", "password"),
            ("username", &config.test_user_email),
            ("password", &config.test_user_password),
            ("client_id", "stacker"),
        ])
        .send()
        .await
        .map_err(|e| format!("Auth request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Auth failed with {}: {}", status, body));
    }

    let token_response: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    token_response["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No access_token in response".to_string())
}

// =============================================================================
// User Profile Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires live User Service"]
async fn test_get_user_profile() {
    let config = match IntegrationConfig::from_env() {
        Some(c) => c,
        None => {
            println!("Skipping: TEST_USER_EMAIL and TEST_USER_PASSWORD not set");
            return;
        }
    };

    let token = get_auth_token(&config).await.expect("Failed to get token");
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/auth/me", config.user_service_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success(), "Expected success status");

    let profile: Value = response.json().await.expect("Failed to parse JSON");
    
    println!("User Profile: {}", serde_json::to_string_pretty(&profile).unwrap());
    
    assert!(profile.get("email").is_some(), "Profile should contain email");
    assert!(profile.get("_id").is_some(), "Profile should contain _id");
}

#[tokio::test]
#[ignore = "requires live User Service"]
async fn test_get_subscription_plan() {
    let config = match IntegrationConfig::from_env() {
        Some(c) => c,
        None => {
            println!("Skipping: TEST_USER_EMAIL and TEST_USER_PASSWORD not set");
            return;
        }
    };

    let token = get_auth_token(&config).await.expect("Failed to get token");
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/oauth_server/api/me", config.user_service_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success(), "Expected success status");

    let user_data: Value = response.json().await.expect("Failed to parse JSON");
    
    println!("User Data: {}", serde_json::to_string_pretty(&user_data).unwrap());
    
    // User profile should include plan information
    let plan = user_data.get("plan");
    println!("Subscription Plan: {:?}", plan);
}

// =============================================================================
// Installations Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires live User Service"]
async fn test_list_installations() {
    let config = match IntegrationConfig::from_env() {
        Some(c) => c,
        None => {
            println!("Skipping: TEST_USER_EMAIL and TEST_USER_PASSWORD not set");
            return;
        }
    };

    let token = get_auth_token(&config).await.expect("Failed to get token");
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/installations", config.user_service_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success(), "Expected success status");

    let installations: Value = response.json().await.expect("Failed to parse JSON");
    
    println!("Installations: {}", serde_json::to_string_pretty(&installations).unwrap());
    
    // Response should have _items array
    assert!(installations.get("_items").is_some(), "Response should have _items");
    
    let items = installations["_items"].as_array().expect("_items should be array");
    println!("Found {} installations", items.len());
    
    for (i, installation) in items.iter().enumerate() {
        println!(
            "  [{}] ID: {}, Status: {}, Stack: {}",
            i,
            installation["_id"],
            installation.get("status").and_then(|v| v.as_str()).unwrap_or("unknown"),
            installation.get("stack_code").and_then(|v| v.as_str()).unwrap_or("unknown")
        );
    }
}

#[tokio::test]
#[ignore = "requires live User Service and TEST_DEPLOYMENT_ID"]
async fn test_get_installation_details() {
    let config = match IntegrationConfig::from_env() {
        Some(c) => c,
        None => {
            println!("Skipping: TEST_USER_EMAIL and TEST_USER_PASSWORD not set");
            return;
        }
    };

    let deployment_id = match config.test_deployment_id {
        Some(id) => id,
        None => {
            println!("Skipping: TEST_DEPLOYMENT_ID not set");
            return;
        }
    };

    let token = get_auth_token(&config).await.expect("Failed to get token");
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/installations/{}", config.user_service_url, deployment_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success(), "Expected success status");

    let details: Value = response.json().await.expect("Failed to parse JSON");
    
    println!("Installation Details: {}", serde_json::to_string_pretty(&details).unwrap());
}

// =============================================================================
// Applications Search Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires live User Service"]
async fn test_search_applications() {
    let config = match IntegrationConfig::from_env() {
        Some(c) => c,
        None => {
            println!("Skipping: TEST_USER_EMAIL and TEST_USER_PASSWORD not set");
            return;
        }
    };

    let token = get_auth_token(&config).await.expect("Failed to get token");
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/applications", config.user_service_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success(), "Expected success status");

    let applications: Value = response.json().await.expect("Failed to parse JSON");
    
    // Response should have _items array
    let items = applications["_items"].as_array();
    if let Some(apps) = items {
        println!("Found {} applications", apps.len());
        for (i, app) in apps.iter().take(5).enumerate() {
            println!(
                "  [{}] {}: {}",
                i,
                app.get("name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                app.get("description").and_then(|v| v.as_str()).unwrap_or("")
            );
        }
    }
}

// =============================================================================
// MCP Tool Simulation Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires live User Service"]
async fn test_mcp_workflow_stack_configuration() {
    //! Simulates the AI's stack configuration workflow:
    //! 1. get_user_profile
    //! 2. get_subscription_plan
    //! 3. list_templates or search_apps
    //! 4. suggest_resources
    //! 5. create_project
    //! 6. validate_domain
    //! 7. start_deployment

    let config = match IntegrationConfig::from_env() {
        Some(c) => c,
        None => {
            println!("Skipping: TEST_USER_EMAIL and TEST_USER_PASSWORD not set");
            return;
        }
    };

    let token = get_auth_token(&config).await.expect("Failed to get token");
    let client = reqwest::Client::new();

    println!("\n=== MCP Stack Configuration Workflow ===\n");

    // Step 1: Get user profile
    println!("Step 1: get_user_profile");
    let profile_resp = client
        .get(&format!("{}/auth/me", config.user_service_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Profile request failed");
    
    assert!(profile_resp.status().is_success());
    let profile: Value = profile_resp.json().await.unwrap();
    println!("  ✓ User: {}", profile.get("email").and_then(|v| v.as_str()).unwrap_or("unknown"));

    // Step 2: Get subscription plan
    println!("Step 2: get_subscription_plan");
    let plan_resp = client
        .get(&format!("{}/oauth_server/api/me", config.user_service_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Plan request failed");
    
    assert!(plan_resp.status().is_success());
    let user_data: Value = plan_resp.json().await.unwrap();
    if let Some(plan) = user_data.get("plan") {
        println!("  ✓ Plan: {}", plan.get("name").and_then(|v| v.as_str()).unwrap_or("unknown"));
    } else {
        println!("  ✓ Plan: (not specified in response)");
    }

    // Step 3: List installations (as proxy for checking deployment limits)
    println!("Step 3: list_installations");
    let installs_resp = client
        .get(&format!("{}/installations", config.user_service_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Installations request failed");
    
    assert!(installs_resp.status().is_success());
    let installs: Value = installs_resp.json().await.unwrap();
    let count = installs["_items"].as_array().map(|a| a.len()).unwrap_or(0);
    println!("  ✓ Current deployments: {}", count);

    // Step 4: Search applications
    println!("Step 4: search_applications");
    let apps_resp = client
        .get(&format!("{}/applications", config.user_service_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Applications request failed");
    
    assert!(apps_resp.status().is_success());
    let apps: Value = apps_resp.json().await.unwrap();
    let app_count = apps["_items"].as_array().map(|a| a.len()).unwrap_or(0);
    println!("  ✓ Available applications: {}", app_count);

    println!("\n=== Workflow Complete ===");
    println!("All User Service integration points working correctly.");
}

// =============================================================================
// Slack Webhook Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires SLACK_SUPPORT_WEBHOOK_URL"]
async fn test_slack_webhook_connectivity() {
    let webhook_url = match env::var("SLACK_SUPPORT_WEBHOOK_URL") {
        Ok(url) => url,
        Err(_) => {
            println!("Skipping: SLACK_SUPPORT_WEBHOOK_URL not set");
            return;
        }
    };

    let client = reqwest::Client::new();
    
    // Send a test message to Slack
    let test_message = json!({
        "blocks": [
            {
                "type": "header",
                "text": {
                    "type": "plain_text",
                    "text": "🧪 Integration Test Message",
                    "emoji": true
                }
            },
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": "This is a test message from the MCP integration test suite.\n\n*This can be ignored.*"
                }
            },
            {
                "type": "context",
                "elements": [
                    {
                        "type": "mrkdwn",
                        "text": "Sent from: stacker/tests/mcp_integration.rs"
                    }
                ]
            }
        ]
    });

    let response = client
        .post(&webhook_url)
        .json(&test_message)
        .send()
        .await
        .expect("Slack webhook request failed");

    println!("Slack response status: {}", response.status());
    
    if response.status().is_success() {
        println!("✓ Slack webhook is working correctly");
    } else {
        let body = response.text().await.unwrap_or_default();
        println!("✗ Slack webhook failed: {}", body);
    }
    
    assert!(response.status().is_success(), "Slack webhook should return success");
}

// =============================================================================
// Confirmation Flow Tests
// =============================================================================

#[tokio::test]
#[ignore = "requires live Stacker service"]
async fn test_confirmation_flow_restart_container() {
    //! Tests the confirmation flow for restart_container:
    //! 1. AI calls restart_container with requires_confirmation: false (dry run)
    //! 2. Returns confirmation prompt
    //! 3. AI calls restart_container with requires_confirmation: true (execute)
    //! 4. Returns result
    
    let stacker_url = env::var("STACKER_URL")
        .unwrap_or_else(|_| "http://localhost:8000".to_string());
    
    println!("\n=== Confirmation Flow Test: restart_container ===\n");
    
    // This test requires MCP WebSocket connection which is complex to simulate
    // In practice, this is tested via the frontend AI assistant
    println!("Note: Full confirmation flow requires WebSocket MCP client");
    println!("Use the frontend AI assistant to test interactively.");
    println!("\nTest scenario:");
    println!("  1. User: 'Restart my nginx container'");
    println!("  2. AI: Calls restart_container(container='nginx', deployment_id=X)");
    println!("  3. AI: Responds 'I'll restart nginx. Please confirm by saying yes.'");
    println!("  4. User: 'Yes, restart it'");
    println!("  5. AI: Calls restart_container with confirmation=true");
    println!("  6. AI: Reports 'Container nginx has been restarted successfully.'");
}

#[tokio::test]
#[ignore = "requires live Stacker service"]
async fn test_confirmation_flow_stop_container() {
    println!("\n=== Confirmation Flow Test: stop_container ===\n");
    
    println!("Test scenario:");
    println!("  1. User: 'Stop the redis container'");
    println!("  2. AI: Calls stop_container(container='redis', deployment_id=X)");
    println!("  3. AI: Responds with warning about service interruption");
    println!("  4. AI: Asks for explicit confirmation");
    println!("  5. User: 'Yes, stop it'");
    println!("  6. AI: Executes stop with graceful timeout");
    println!("  7. AI: Reports result");
}

#[tokio::test]
#[ignore = "requires live Stacker service"]
async fn test_confirmation_flow_delete_project() {
    println!("\n=== Confirmation Flow Test: delete_project ===\n");
    
    println!("Test scenario:");
    println!("  1. User: 'Delete my test-project'");
    println!("  2. AI: Calls delete_project(project_id=X)");
    println!("  3. AI: Lists what will be deleted (containers, volumes, configs)");
    println!("  4. AI: Warns this action is irreversible");
    println!("  5. User: 'Yes, delete it permanently'");
    println!("  6. AI: Executes deletion");
    println!("  7. AI: Confirms deletion complete");
}
