mod common;

use serde_json::{json, Value};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, ResponseTemplate};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Vault path pattern for SSH keys: /v1/secret/users/{user_id}/ssh_keys/{server_id}
fn vault_ssh_path_regex(user_id: &str, server_id: i32) -> String {
    format!(
        r"/v1/secret/users/{}/ssh_keys/{}",
        user_id, server_id
    )
}

/// Successful Vault GET response body for a KV v1 SSH key read.
fn vault_key_response(public_key: &str, private_key: &str) -> serde_json::Value {
    json!({
        "data": {
            "public_key": public_key,
            "private_key": private_key
        }
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: GET /server/{id}/ssh-key/public
// ─────────────────────────────────────────────────────────────────────────────

/// Server has key_status=active but vault_key_path=NULL (Vault failed during generate).
/// Must return 400, not 500.
#[tokio::test]
async fn test_get_public_key_vault_path_null_returns_400() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        "test_user_id",
        project_id,
        "active",
        None, // vault_key_path is NULL
    )
    .await;

    let client = reqwest::Client::new();
    let resp = client
        .get(&format!("{}/server/{}/ssh-key/public", &app.address, server_id))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status().as_u16(), 400, "Should be 400, not 500");
    let body: Value = resp.json().await.unwrap();
    let msg = body["message"].as_str().unwrap_or("");
    assert!(
        msg.to_lowercase().contains("vault") || msg.to_lowercase().contains("regenerate") || msg.to_lowercase().contains("delete"),
        "Error message should mention Vault or remediation: {}", msg
    );
    // Vault server must NOT have been called (no vault_key_path to use)
    assert_eq!(app.vault_server.received_requests().await.unwrap().len(), 0);
}

/// Server has key_status=active and vault_key_path set, but Vault returns 404.
/// Must return 404 (key lost from Vault), not 500.
#[tokio::test]
async fn test_get_public_key_vault_returns_404_propagates_as_404() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        "test_user_id",
        project_id,
        "active",
        Some(&format!("secret/users/test_user_id/ssh_keys/{}", 999)),
    )
    .await;

    // Mount Vault mock: GET → 404
    Mock::given(method("GET"))
        .and(path_regex(vault_ssh_path_regex("test_user_id", server_id)))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"errors": []})))
        .mount(&app.vault_server)
        .await;

    let client = reqwest::Client::new();
    let resp = client
        .get(&format!("{}/server/{}/ssh-key/public", &app.address, server_id))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status().as_u16(), 404, "Should be 404 when Vault returns 404");
    let body: Value = resp.json().await.unwrap();
    let msg = body["message"].as_str().unwrap_or("");
    assert!(
        msg.to_lowercase().contains("vault") || msg.to_lowercase().contains("regenerate"),
        "Error message should mention Vault: {}", msg
    );
}

/// Server has key_status="none" — no key has been generated yet.
/// Must return 404.
#[tokio::test]
async fn test_get_public_key_no_active_key_returns_404() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        "test_user_id",
        project_id,
        "none",
        None,
    )
    .await;

    let client = reqwest::Client::new();
    let resp = client
        .get(&format!("{}/server/{}/ssh-key/public", &app.address, server_id))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status().as_u16(), 404);
}

/// Happy path: active key, vault_key_path set, Vault returns the key successfully.
#[tokio::test]
async fn test_get_public_key_success() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        "test_user_id",
        project_id,
        "active",
        Some(&format!("secret/users/test_user_id/ssh_keys/{}", 0)), // path value doesn't matter for routing
    )
    .await;

    let expected_pub_key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITestPublicKey";

    Mock::given(method("GET"))
        .and(path_regex(vault_ssh_path_regex("test_user_id", server_id)))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(vault_key_response(
                expected_pub_key,
                "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----",
            )),
        )
        .mount(&app.vault_server)
        .await;

    let client = reqwest::Client::new();
    let resp = client
        .get(&format!("{}/server/{}/ssh-key/public", &app.address, server_id))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status().as_u16(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["item"]["public_key"].as_str().unwrap_or(""),
        expected_pub_key,
        "Response should contain the public key from Vault"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: POST /server/{id}/ssh-key/generate
// ─────────────────────────────────────────────────────────────────────────────

/// When Vault is unavailable during generate, the private key MUST be returned
/// inline in the response, and the DB must have key_status=active + vault_key_path=NULL.
#[tokio::test]
async fn test_generate_key_vault_down_returns_private_key_inline() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        "test_user_id",
        project_id,
        "none",
        None,
    )
    .await;

    // Vault is down — POST returns 500
    Mock::given(method("POST"))
        .and(path_regex(vault_ssh_path_regex("test_user_id", server_id)))
        .respond_with(ResponseTemplate::new(500).set_body_string("vault unavailable"))
        .mount(&app.vault_server)
        .await;

    let client = reqwest::Client::new();
    let resp = client
        .post(&format!("{}/server/{}/ssh-key/generate", &app.address, server_id))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status().as_u16(), 200, "Generate should succeed even when Vault is down");
    let body: Value = resp.json().await.unwrap();

    // Private key must be returned inline so user can save it
    assert!(
        body["item"]["private_key"].is_string(),
        "Private key must be returned inline when Vault is unavailable"
    );
    assert!(
        body["item"]["public_key"].is_string(),
        "Public key must also be present"
    );

    // DB: key_status must be "active" and vault_key_path must be NULL
    let row = sqlx::query("SELECT key_status, vault_key_path FROM server WHERE id = $1")
        .bind(server_id)
        .fetch_one(&app.db_pool)
        .await
        .expect("DB query failed");
    use sqlx::Row;
    let db_key_status: String = row.get("key_status");
    let db_vault_path: Option<String> = row.get("vault_key_path");
    assert_eq!(db_key_status, "active");
    assert!(
        db_vault_path.is_none(),
        "vault_key_path must be NULL when Vault store failed"
    );
}

/// Happy path: Vault is available — key is stored, no private key in response, vault_key_path saved.
#[tokio::test]
async fn test_generate_key_success_stores_in_vault_no_private_key_exposed() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        "test_user_id",
        project_id,
        "none",
        None,
    )
    .await;

    // Vault is up — POST returns 204
    Mock::given(method("POST"))
        .and(path_regex(vault_ssh_path_regex("test_user_id", server_id)))
        .respond_with(ResponseTemplate::new(204))
        .mount(&app.vault_server)
        .await;

    let client = reqwest::Client::new();
    let resp = client
        .post(&format!("{}/server/{}/ssh-key/generate", &app.address, server_id))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status().as_u16(), 200);
    let body: Value = resp.json().await.unwrap();

    // Private key must NOT be in response when Vault worked
    assert!(
        body["item"]["private_key"].is_null() || !body["item"]["private_key"].is_string(),
        "Private key must NOT be returned when Vault stored it successfully"
    );
    assert!(body["item"]["public_key"].is_string(), "Public key must be present");

    // DB: vault_key_path must be set
    let row = sqlx::query("SELECT key_status, vault_key_path FROM server WHERE id = $1")
        .bind(server_id)
        .fetch_one(&app.db_pool)
        .await
        .expect("DB query failed");
    use sqlx::Row;
    let db_key_status: String = row.get("key_status");
    let db_vault_path: Option<String> = row.get("vault_key_path");
    assert_eq!(db_key_status, "active");
    assert!(
        db_vault_path.is_some(),
        "vault_key_path must be saved in DB after successful Vault store"
    );
}

/// Generating a key when one is already active must return 400.
#[tokio::test]
async fn test_generate_key_already_active_returns_400() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        "test_user_id",
        project_id,
        "active",
        Some("secret/users/test_user_id/ssh_keys/1"),
    )
    .await;

    let client = reqwest::Client::new();
    let resp = client
        .post(&format!("{}/server/{}/ssh-key/generate", &app.address, server_id))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status().as_u16(), 400);
    // Vault must NOT have been called
    assert_eq!(app.vault_server.received_requests().await.unwrap().len(), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: DELETE /server/{id}/ssh-key
// ─────────────────────────────────────────────────────────────────────────────

/// Deleting an active key must call Vault DELETE, reset key_status to "none",
/// and clear vault_key_path in DB.
#[tokio::test]
async fn test_delete_key_clears_vault_and_db() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        "test_user_id",
        project_id,
        "active",
        Some(&format!("secret/users/test_user_id/ssh_keys/{}", 0)),
    )
    .await;

    Mock::given(method("DELETE"))
        .and(path_regex(vault_ssh_path_regex("test_user_id", server_id)))
        .respond_with(ResponseTemplate::new(204))
        .mount(&app.vault_server)
        .await;

    let client = reqwest::Client::new();
    let resp = client
        .delete(&format!("{}/server/{}/ssh-key", &app.address, server_id))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status().as_u16(), 200);

    let row = sqlx::query("SELECT key_status, vault_key_path FROM server WHERE id = $1")
        .bind(server_id)
        .fetch_one(&app.db_pool)
        .await
        .expect("DB query failed");
    use sqlx::Row;
    let db_key_status: String = row.get("key_status");
    let db_vault_path: Option<String> = row.get("vault_key_path");
    assert_eq!(db_key_status, "none", "key_status must be reset to 'none'");
    assert!(db_vault_path.is_none(), "vault_key_path must be cleared");
}

/// Deleting when no key exists must return 400.
#[tokio::test]
async fn test_delete_key_none_returns_400() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        "test_user_id",
        project_id,
        "none",
        None,
    )
    .await;

    let client = reqwest::Client::new();
    let resp = client
        .delete(&format!("{}/server/{}/ssh-key", &app.address, server_id))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status().as_u16(), 400);
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: Unauthenticated access
// ─────────────────────────────────────────────────────────────────────────────

/// All SSH key endpoints must reject requests without a Bearer token.
#[tokio::test]
async fn test_ssh_key_endpoints_require_auth() {
    let app = match common::spawn_app_with_vault().await {
        Some(a) => a,
        None => return,
    };
    let client = reqwest::Client::new();

    let endpoints: &[(&str, &str)] = &[
        ("GET",    "/server/1/ssh-key/public"),
        ("POST",   "/server/1/ssh-key/generate"),
        ("DELETE", "/server/1/ssh-key"),
    ];

    for (verb, path) in endpoints {
        let req = match *verb {
            "GET"    => client.get(&format!("{}{}", &app.address, path)),
            "POST"   => client.post(&format!("{}{}", &app.address, path)),
            "DELETE" => client.delete(&format!("{}{}", &app.address, path)),
            _        => unreachable!(),
        };
        let resp = req.send().await.expect("request failed");
        let status = resp.status().as_u16();
        assert!(
            status == 400 || status == 401 || status == 403 || status == 404,
            "{} {} without auth should return 400/401/403, got {}",
            verb, path, status
        );
    }
}
