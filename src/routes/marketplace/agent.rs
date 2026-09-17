//! Agent registration for marketplace purchases — **not implemented**.
//!
//! The intended flow: a user buys a stack in the marketplace, an agent comes
//! up on their server and registers with the *purchase token* rather than by
//! signing in to the dashboard. Nothing behind that exists yet.
//!
//! The handler used to answer `201` with a freshly generated `agent_id`,
//! `agent_token` and `deployment_hash` while writing none of them — no agent
//! row, no deployment, no Vault entry. The token was inert: an agent
//! presenting it would be rejected as unknown on its first request. That is a
//! worse failure than an honest refusal, because the response is
//! indistinguishable from a working registration, so it now answers `501`.
//!
//! Whoever implements this: mint through `services::agent_token::issue`, which
//! writes the digest and the Vault value together. Do not add another local
//! generator — this module carried a fourth copy of one, removed with the
//! stub.

use actix_web::{post, HttpResponse, Result};

/// Request shape the marketplace installer is expected to send. Kept as the
/// record of the intended contract; nothing parses it while the endpoint is
/// unimplemented.
#[derive(Debug, serde::Deserialize)]
pub struct AgentRegisterRequest {
    pub purchase_token: String,
    pub server_fingerprint: serde_json::Value,
    pub stack_id: String,
}

/// Response shape the caller is expected to receive once implemented.
#[derive(Debug, serde::Serialize)]
pub struct AgentRegisterResponse {
    pub agent_id: String,
    pub agent_token: String,
    pub deployment_hash: String,
    pub dashboard_url: String,
}

/// Still to be built:
///
/// 1. validate the purchase token with User Service
///    (`POST /marketplace/purchase-token/validate`);
/// 2. create the agent record, issuing its token via
///    `services::agent_token::issue`;
/// 3. create the deployment record;
/// 4. call User Service `/marketplace/link-deployment`.
///
/// The body is deliberately not extracted: rejecting a malformed payload with
/// `400` would imply the endpoint does something with it. It does not.
/// `purchase_token` is also no longer logged — it is a credential.
#[tracing::instrument(name = "Register marketplace agent", skip_all)]
#[post("/register")]
pub async fn register_marketplace_agent_handler() -> Result<HttpResponse> {
    tracing::warn!("Marketplace agent registration called, but it is not implemented");

    Ok(HttpResponse::NotImplemented().json(serde_json::json!({
        "message": "Marketplace agent registration is not implemented"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    /// Pins the refusal. The hazard being guarded against is a future edit
    /// restoring a `201` with a token that nothing recorded — a response that
    /// looks successful and hands out a credential no agent can authenticate
    /// with.
    #[actix_web::test]
    async fn marketplace_agent_registration_is_refused() {
        let app = test::init_service(App::new().service(register_marketplace_agent_handler)).await;

        let req = test::TestRequest::post()
            .uri("/register")
            .set_json(serde_json::json!({
                "purchase_token": "pt_example",
                "server_fingerprint": {},
                "stack_id": "stack_example"
            }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 501, "the endpoint must not claim success");

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert!(
            body.get("agent_token").is_none(),
            "no credential may be handed out: {body}"
        );
    }
}
