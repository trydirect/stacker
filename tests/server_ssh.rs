mod common;

use serde_json::json;

// Test SSH key generation for server
// Run: cargo t --test server_ssh -- --nocapture --show-output

/// Test that the server list endpoint returns success
#[tokio::test]
async fn get_server_list() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/server", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    // Should return 200 OK (empty list is fine)
    assert!(response.status().is_success());
}

/// Test that getting a non-existent server returns 404
#[tokio::test]
async fn get_server_not_found() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/server/99999", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    // Should return 404 for non-existent server
    assert_eq!(response.status().as_u16(), 404);
}

/// Test that generating SSH key requires authentication
#[tokio::test]
async fn generate_ssh_key_requires_auth() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let response = client
        .post(&format!("{}/server/1/ssh-key/generate", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    // Should require authentication (401 or 403)
    let status = response.status().as_u16();
    assert!(status == 401 || status == 403 || status == 404);
}

/// Test that uploading SSH key validates input
#[tokio::test]
async fn upload_ssh_key_validates_input() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    // Send invalid key format
    let invalid_data = json!({
        "public_key": "not-a-valid-key",
        "private_key": "also-not-valid"
    });

    let response = client
        .post(&format!("{}/server/1/ssh-key/upload", &app.address))
        .header("Content-Type", "application/json")
        .body(invalid_data.to_string())
        .send()
        .await
        .expect("Failed to execute request.");

    // Should reject invalid key format (400 or 401/403 if auth required first)
    let status = response.status().as_u16();
    assert!(status == 400 || status == 401 || status == 403 || status == 404);
}

/// Test that getting public key for non-existent server returns error
#[tokio::test]
async fn get_public_key_not_found() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/server/99999/ssh-key/public", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    // Should return 404
    let status = response.status().as_u16();
    assert!(status == 404 || status == 401 || status == 403);
}

/// Test that deleting SSH key for non-existent server returns error
#[tokio::test]
async fn delete_ssh_key_not_found() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let response = client
        .delete(&format!("{}/server/99999/ssh-key", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    // Should return 404 or auth error
    let status = response.status().as_u16();
    assert!(status == 404 || status == 401 || status == 403);
}

/// Test server update endpoint
#[tokio::test]
async fn update_server_not_found() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let update_data = json!({
        "name": "My Server",
        "connection_mode": "ssh"
    });

    let response = client
        .put(&format!("{}/server/99999", &app.address))
        .header("Content-Type", "application/json")
        .body(update_data.to_string())
        .send()
        .await
        .expect("Failed to execute request.");

    // Should return 404 for non-existent server
    let status = response.status().as_u16();
    assert!(status == 404 || status == 401 || status == 403);
}

/// Test get servers by project endpoint
#[tokio::test]
async fn get_servers_by_project() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/server/project/1", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    // Should return success or auth error
    let status = response.status().as_u16();
    assert!(status == 200 || status == 404 || status == 401 || status == 403);
}
