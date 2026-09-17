//! A stateful in-memory stand-in for a Vault KV v1 mount.
//!
//! Shared between the integration harness (`tests/common/mod.rs`) and the BDD
//! harness (`tests/steps/common.rs`) via `#[path]` include, because Rust test
//! binaries do not share a module tree and this was previously about to become
//! a second byte-for-byte copy.
#![allow(dead_code)]

/// `POST` stores the request body under the request path, `GET` returns it
/// wrapped as `{"data": <body>}` — exactly what a real KV v1 mount does. That
/// statefulness matters: agent registration writes the freshly minted token
/// via `store_agent_token`, and authentication reads it straight back, so a
/// fixed canned response cannot serve both halves of the flow.
///
/// Tests used to run with `vault.address` pointing at a dead `127.0.0.1:8200`.
/// Agent auth relied on a fallback that substituted the presented bearer token
/// for the stored one — an auth bypass, since removed. Registration relied on
/// the Vault write being a detached `spawn`, so a failed store still answered
/// `201`; that is now awaited. Both mean a test touching agent credentials
/// needs a Vault that actually answers.
#[derive(Clone, Default)]
pub struct VaultKvMock {
    store: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>>,
}

impl wiremock::Respond for VaultKvMock {
    fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let key = request.url.path().to_string();

        match request.method {
            wiremock::http::Method::Post | wiremock::http::Method::Put => {
                let body: serde_json::Value =
                    serde_json::from_slice(&request.body).unwrap_or(serde_json::Value::Null);
                self.store.lock().unwrap().insert(key, body);
                wiremock::ResponseTemplate::new(204)
            }
            wiremock::http::Method::Delete => {
                self.store.lock().unwrap().remove(&key);
                wiremock::ResponseTemplate::new(204)
            }
            _ => match self.store.lock().unwrap().get(&key) {
                Some(value) => wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "data": value })),
                None => wiremock::ResponseTemplate::new(404),
            },
        }
    }
}

/// Mount [`VaultKvMock`] on `server` so it behaves like a KV v1 mount for any
/// path. Call before the app issues its first Vault request.
pub async fn mount_vault_kv_mock(server: &wiremock::MockServer) {
    let backing = VaultKvMock::default();
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(backing)
        .mount(server)
        .await;
}
