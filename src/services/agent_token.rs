//! Issuing agent bearer tokens.
//!
//! The single place a token is minted. Every path that hands an agent a
//! credential — first registration, re-registration, link, re-link, operator
//! rotation — goes through [`issue`], so the digest in Postgres and the value
//! in Vault cannot drift apart.
//!
//! That matters because the agent polls Vault and adopts whatever it finds
//! (`status`, `src/security/token_refresh.rs`). A write to Vault that skipped
//! the database would hand the agent a token Stacker can never verify, and the
//! agent would lock itself out permanently on its next refresh.

use crate::db;
use crate::helpers::agent_token;
use crate::helpers::vault::VaultClient;
use sqlx::PgPool;
use uuid::Uuid;

/// Mint a token, record its digest, publish it to Vault, and return it.
///
/// Both writes are awaited. Callers must not return the token to the agent
/// unless this succeeds: before, all four call sites stored to Vault in a
/// detached `actix_web::rt::spawn` and answered immediately, so a failed store
/// left the agent holding a credential nothing could verify — and, less
/// visibly, a poll arriving before the spawned write completed was rejected
/// with "Token not found in Vault". That race is why
/// `tests/agent_command_flow.rs` failed intermittently.
///
/// **Order is deliberate: database first, then Vault.** If Vault fails the
/// caller errors out and the agent receives nothing; the orphaned digest is
/// inert and is overwritten on the next attempt. Reversed, Vault would hold a
/// token Stacker cannot verify and the agent's refresh loop would adopt it.
/// Failing toward "no credential" beats failing toward "poisoned credential".
#[tracing::instrument(name = "Issue agent token", skip(pool, vault), fields(deployment_hash = %deployment_hash))]
pub async fn issue(
    pool: &PgPool,
    vault: &VaultClient,
    agent_id: Uuid,
    deployment_hash: &str,
) -> Result<String, String> {
    let token = agent_token::generate();

    db::agent::set_token_hash(pool, agent_id, &agent_token::hash(&token)).await?;

    store_with_retry(vault, deployment_hash, &token).await?;

    tracing::info!(
        agent_id = %agent_id,
        "Issued agent token: digest stored and value published to Vault"
    );
    Ok(token)
}

/// Publish to Vault, retrying transient failures.
///
/// Keeps the three-attempt exponential backoff the previous fire-and-forget
/// stores used, but awaited, so the outcome reaches the caller.
async fn store_with_retry(
    vault: &VaultClient,
    deployment_hash: &str,
    token: &str,
) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 0..3 {
        match vault.store_agent_token(deployment_hash, token).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = err;
                tracing::warn!(
                    attempt = attempt + 1,
                    error = %last_err,
                    "Vault store failed while issuing agent token"
                );
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_secs(2_u64.pow(attempt))).await;
                }
            }
        }
    }
    Err(format!(
        "Failed to publish agent token to Vault after 3 attempts: {last_err}"
    ))
}
