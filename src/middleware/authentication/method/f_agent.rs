use crate::helpers::AgentPgPool;
use crate::middleware::authentication::get_header;
use crate::models;
use actix_web::{dev::ServiceRequest, web, HttpMessage};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;
use uuid::Uuid;

async fn log_audit(
    db_pool: PgPool,
    agent_id: Option<Uuid>,
    deployment_hash: Option<String>,
    action: String,
    status: String,
    details: serde_json::Value,
) {
    let query_span = tracing::info_span!("Logging agent audit event");

    let result = sqlx::query(
        r#"
        INSERT INTO audit_log (agent_id, deployment_hash, action, status, details, created_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        "#,
    )
    .bind(agent_id)
    .bind(deployment_hash)
    .bind(action)
    .bind(status)
    .bind(details)
    .execute(&db_pool)
    .instrument(query_span)
    .await;

    if let Err(e) = result {
        tracing::error!("Failed to log audit event: {:?}", e);
    }
}

#[tracing::instrument(name = "Authenticate agent via X-Agent-Id and Bearer token")]
pub async fn try_agent(req: &mut ServiceRequest) -> Result<bool, String> {
    // Check for X-Agent-Id header
    let agent_id_header = get_header::<String>(req, "x-agent-id")?;
    if agent_id_header.is_none() {
        return Ok(false);
    }

    let agent_id_str = agent_id_header.unwrap();
    let agent_id =
        Uuid::parse_str(&agent_id_str).map_err(|_| "Invalid agent ID format".to_string())?;

    // Check for Authorization header
    let auth_header = get_header::<String>(req, "authorization")?;
    if auth_header.is_none() {
        return Err("Authorization header required for agent".to_string());
    }

    let bearer_token = auth_header
        .unwrap()
        .strip_prefix("Bearer ")
        .ok_or("Invalid Authorization header format")?
        .to_string();

    // Get agent database pool (separate pool for agent operations)
    let agent_pool = req
        .app_data::<web::Data<AgentPgPool>>()
        .ok_or("Agent database pool not found")?;
    let db_pool: &PgPool = agent_pool.get_ref().as_ref();

    // Fetch agent from database
    // `db::agent::fetch_by_id` rather than a local copy: this module used to
    // carry a fourth hand-rolled SELECT of the agent columns, which is the
    // easiest place to miss when the table gains one.
    let agent = crate::db::agent::fetch_by_id(db_pool, agent_id)
        .await?
        .ok_or_else(|| "Agent not found".to_string())?;

    // Verify against the stored digest, not against a secret read back from
    // Vault. Authentication no longer needs `read` on any agent's token, so
    // compromising Stacker's Vault token no longer leaks every agent's
    // credential at once. Vault remains the *distribution* channel — the agent
    // polls it and adopts rotations — which is why every mint goes through
    // `services::agent_token::issue`, writing digest and Vault together.
    let Some(stored_hash) = agent.token_hash.as_deref() else {
        // Fails closed. A row without a digest predates this scheme or was
        // written by a path that bypassed `issue`; either way there is nothing
        // to verify against. Recovery is one command:
        // `stacker agent rotate-token --deployment-hash <hash>`.
        actix_web::rt::spawn(log_audit(
            agent_pool.inner().clone(),
            Some(agent_id),
            Some(agent.deployment_hash.clone()),
            "agent.auth_failure".to_string(),
            "token_hash_missing".to_string(),
            serde_json::json!({}),
        ));
        return Err("Agent credential not provisioned".to_string());
    };

    if !crate::helpers::agent_token::verify(&bearer_token, stored_hash) {
        actix_web::rt::spawn(log_audit(
            agent_pool.inner().clone(),
            Some(agent_id),
            Some(agent.deployment_hash.clone()),
            "agent.auth_failure".to_string(),
            "token_mismatch".to_string(),
            serde_json::json!({}),
        ));
        return Err("Invalid agent token".to_string());
    }

    // Token matches, set up access control
    let acl_vals = actix_casbin_auth::CasbinVals {
        subject: "agent".to_string(),
        domain: None,
    };

    // Create a pseudo-user for agent (for compatibility with existing handlers)
    let agent_user = models::User {
        id: agent.deployment_hash.clone(), // Use deployment_hash as user_id
        role: "agent".to_string(),
        first_name: "Agent".to_string(),
        last_name: format!("#{}", &agent.id.to_string()[..8]), // First 8 chars of UUID
        email: format!("agent+{}@system.local", agent.deployment_hash),
        email_confirmed: true,
        mfa_verified: false,
        access_token: None,
    };

    if req.extensions_mut().insert(Arc::new(agent_user)).is_some() {
        return Err("Agent already authenticated".to_string());
    }

    if req
        .extensions_mut()
        .insert(Arc::new(agent.clone()))
        .is_some()
    {
        return Err("Agent data already set".to_string());
    }

    if req.extensions_mut().insert(acl_vals).is_some() {
        return Err("Access control already set".to_string());
    }

    // Log successful authentication
    actix_web::rt::spawn(log_audit(
        db_pool.clone(),
        Some(agent_id),
        Some(agent.deployment_hash.clone()),
        "agent.auth_success".to_string(),
        "success".to_string(),
        serde_json::json!({}),
    ));

    tracing::debug!(
        "Agent authenticated: {} ({})",
        agent_id,
        agent.deployment_hash
    );

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::try_agent;
    use actix_web::test::TestRequest;

    /// Agent authentication must fail closed when there is nothing to verify
    /// against.
    ///
    /// This test outlives the code it was written for, deliberately: the
    /// invariant is "no usable credential on record means no access", and only
    /// the mechanism has changed. It previously pinned
    /// `vault_failure_is_tolerable`, a predicate that gated a Vault-unreachable
    /// fallback. That fallback substituted the *presented* bearer token for the
    /// stored one, so `bearer_token != stored_token` compared a value against
    /// itself and always passed: any string authenticated as any agent, given
    /// only its `X-Agent-Id` UUID. It was gated on a substring of
    /// `vault.address` rather than a build flag, so it shipped in release
    /// binaries and armed itself for the common Vault-sidecar layout
    /// (`http://127.0.0.1:8200`); and it triggered on *any* Vault error, so a
    /// policy denial or a malformed response opened it just as wide.
    ///
    /// The same shape of mistake is now available one level down: `token_hash`
    /// is nullable, and treating a missing or unparseable digest as a pass
    /// would reopen exactly that hole. `try_agent` returns
    /// "Agent credential not provisioned" before reaching `verify`; this pins
    /// the layer underneath, that no bearer token authenticates against a
    /// stored value that cannot be interpreted.
    #[test]
    fn null_token_hash_never_authenticates() {
        use crate::helpers::agent_token;

        let plausible = agent_token::generate();
        for stored in [
            // What a NULL column becomes if it is ever unwrapped to a default
            // instead of rejected outright.
            "",
            "   ",
            "null",
            "NULL",
            // Right length, no algorithm tag.
            &agent_token::hash(&plausible)[7..],
            // The tag alone.
            "sha256:",
        ] {
            for presented in ["", &plausible, "anything"] {
                assert!(
                    !agent_token::verify(presented, stored),
                    "{presented:?} must not authenticate against stored value {stored:?}"
                );
            }
        }

        // Note what is deliberately *not* asserted: `verify("", &hash(""))` is
        // true, and correctly so — it is a valid round-trip. The danger is a
        // NULL column being coerced into `hash("")` somewhere upstream, which
        // `verify` cannot see. That is guarded above `verify`, by `try_agent`
        // rejecting `None` before it gets here and by `issue` only ever hashing
        // output of `generate`.

        // The one case that must pass, so the test cannot go green by having
        // `verify` reject everything.
        assert!(agent_token::verify(
            &plausible,
            &agent_token::hash(&plausible)
        ));
    }

    #[actix_web::test]
    async fn no_x_agent_id_header_skips_agent_auth() {
        let mut req = TestRequest::default().to_srv_request();
        let result = try_agent(&mut req).await;
        assert_eq!(result, Ok(false));
    }

    #[actix_web::test]
    async fn invalid_uuid_in_x_agent_id_returns_error() {
        let mut req = TestRequest::default()
            .insert_header(("x-agent-id", "not-a-uuid"))
            .to_srv_request();
        let result = try_agent(&mut req).await;
        assert_eq!(result, Err("Invalid agent ID format".to_string()));
    }

    #[actix_web::test]
    async fn valid_uuid_but_no_authorization_header_returns_error() {
        let mut req = TestRequest::default()
            .insert_header(("x-agent-id", "550e8400-e29b-41d4-a716-446655440000"))
            .to_srv_request();
        let result = try_agent(&mut req).await;
        assert_eq!(
            result,
            Err("Authorization header required for agent".to_string())
        );
    }

    #[actix_web::test]
    async fn valid_uuid_non_bearer_authorization_returns_error() {
        let mut req = TestRequest::default()
            .insert_header(("x-agent-id", "550e8400-e29b-41d4-a716-446655440000"))
            .insert_header(("authorization", "Basic abc123"))
            .to_srv_request();
        let result = try_agent(&mut req).await;
        assert_eq!(
            result,
            Err("Invalid Authorization header format".to_string())
        );
    }
}
