use hmac::{Hmac, Mac};
use reqwest::{Client, Response};
use serde::Serialize;
use serde_json::Value;
use sha2::Sha256;
use base64::Engine;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub struct AgentClient {
    http: Client,
    base_url: String,
    agent_id: String,
    agent_token: String,
}

impl AgentClient {
    pub fn new<S1: Into<String>, S2: Into<String>, S3: Into<String>>(base_url: S1, agent_id: S2, agent_token: S3) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            agent_id: agent_id.into(),
            agent_token: agent_token.into(),
        }
    }

    fn now_unix() -> String {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        ts.to_string()
    }

    fn sign_body(&self, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.agent_token.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(body);
        let bytes = mac.finalize().into_bytes();
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    async fn post_signed_bytes(&self, path: &str, body_bytes: Vec<u8>) -> Result<Response, reqwest::Error> {
        let url = format!("{}{}{}", self.base_url, if path.starts_with('/') { "" } else { "/" }, path);
        let timestamp = Self::now_unix();
        let request_id = Uuid::new_v4().to_string();
        let signature = self.sign_body(&body_bytes);

        self.http
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Agent-Id", &self.agent_id)
            .header("X-Timestamp", timestamp)
            .header("X-Request-Id", request_id)
            .header("X-Agent-Signature", signature)
            .body(body_bytes)
            .send()
            .await
    }

    async fn post_signed_json<T: Serialize>(&self, path: &str, body: &T) -> Result<Response, reqwest::Error> {
        let bytes = serde_json::to_vec(body).expect("serializable body");
        self.post_signed_bytes(path, bytes).await
    }

    // POST /api/v1/commands/execute
    pub async fn commands_execute(&self, payload: &Value) -> Result<Response, reqwest::Error> {
        self.post_signed_json("/api/v1/commands/execute", payload).await
    }

    // POST /api/v1/commands/enqueue
    pub async fn commands_enqueue(&self, payload: &Value) -> Result<Response, reqwest::Error> {
        self.post_signed_json("/api/v1/commands/enqueue", payload).await
    }

    // POST /api/v1/commands/report
    pub async fn commands_report(&self, payload: &Value) -> Result<Response, reqwest::Error> {
        self.post_signed_json("/api/v1/commands/report", payload).await
    }

    // POST /api/v1/auth/rotate-token (signed with current token)
    pub async fn rotate_token(&self, new_token: &str) -> Result<Response, reqwest::Error> {
        #[derive(Serialize)]
        struct RotateBody<'a> { new_token: &'a str }
        let body = RotateBody { new_token };
        self.post_signed_json("/api/v1/auth/rotate-token", &body).await
    }

    // GET /api/v1/commands/wait/{hash} (no signature, only X-Agent-Id)
    pub async fn wait(&self, deployment_hash: &str) -> Result<Response, reqwest::Error> {
        let url = format!(
            "{}/api/v1/commands/wait/{}",
            self.base_url, deployment_hash
        );
        self.http
            .get(url)
            .header("X-Agent-Id", &self.agent_id)
            .send()
            .await
    }
}
