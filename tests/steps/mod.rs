pub mod common;
pub mod health;

use cucumber::World;
use sqlx::PgPool;
use std::collections::HashMap;

/// Shared BDD test world holding the running app and request/response state.
#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct StepWorld {
    /// Base URL of the test server (e.g. "http://127.0.0.1:54321")
    pub base_url: String,
    /// Database connection pool for the test database
    pub db_pool: Option<PgPool>,
    /// HTTP client for making requests
    pub client: reqwest::Client,
    /// Last HTTP response status code
    pub status_code: Option<u16>,
    /// Last HTTP response body as string
    pub response_body: Option<String>,
    /// Auth token for the current user (default: User A)
    pub auth_token: String,
    /// Stored IDs from create operations (e.g. "project_id" -> "42")
    pub stored_ids: HashMap<String, String>,
    /// Last JSON response parsed
    pub response_json: Option<serde_json::Value>,
}

impl StepWorld {
    async fn new() -> Self {
        let app = common::spawn_bdd_app()
            .await
            .expect("BDD: Failed to start test server (is PostgreSQL running?)");

        Self {
            base_url: app.address,
            db_pool: Some(app.db_pool),
            client: reqwest::Client::new(),
            status_code: None,
            response_body: None,
            auth_token: "user-a-token".to_string(),
            stored_ids: HashMap::new(),
            response_json: None,
        }
    }

    /// Make a GET request to the app, returns (status, body)
    pub async fn get(&mut self, path: &str) -> (u16, String) {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await
            .expect("GET request failed");

        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        self.status_code = Some(status);
        self.response_body = Some(body.clone());
        self.response_json = serde_json::from_str(&body).ok();
        (status, body)
    }

    /// Make a POST request with JSON body
    pub async fn post_json(&mut self, path: &str, body: &serde_json::Value) -> (u16, String) {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(body)
            .send()
            .await
            .expect("POST request failed");

        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        self.status_code = Some(status);
        self.response_body = Some(body.clone());
        self.response_json = serde_json::from_str(&body).ok();
        (status, body)
    }

    /// Make a PUT request with JSON body
    pub async fn put_json(&mut self, path: &str, body: &serde_json::Value) -> (u16, String) {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(body)
            .send()
            .await
            .expect("PUT request failed");

        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        self.status_code = Some(status);
        self.response_body = Some(body.clone());
        self.response_json = serde_json::from_str(&body).ok();
        (status, body)
    }

    /// Make a DELETE request
    pub async fn delete(&mut self, path: &str) -> (u16, String) {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await
            .expect("DELETE request failed");

        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        self.status_code = Some(status);
        self.response_body = Some(body.clone());
        self.response_json = serde_json::from_str(&body).ok();
        (status, body)
    }

    /// Store an ID from the last response JSON
    pub fn store_id_from_response(&mut self, key: &str, json_path: &str) {
        if let Some(ref json) = self.response_json {
            if let Some(val) = json.pointer(json_path) {
                let id_str = match val {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                self.stored_ids.insert(key.to_string(), id_str);
            }
        }
    }
}
