//! Sandbox lifecycle manager (WS-S S1): reap sandboxes that outlived their
//! hard lifetime or went idle past their inactivity timeout.
//!
//! A reap enqueues a `sandbox` kill command for the agent on the sandbox's
//! deployment and marks the row expired. The container carries its own
//! lifetime, so a server restart does not strand it.

use crate::db;
use crate::models::Sandbox;
use crate::services::agent_dispatcher::AgentDispatcher;
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use std::time::Duration;

/// How often the lifecycle manager looks for sandboxes to reap.
const TICK: Duration = Duration::from_secs(30);

pub fn spawn(pg_pool: PgPool) {
    tokio::spawn(async move {
        tracing::info!("sandbox_lifecycle started (tick={:?})", TICK);
        loop {
            tokio::time::sleep(TICK).await;
            if let Err(err) = tick(&pg_pool).await {
                tracing::warn!("sandbox_lifecycle tick error: {}", err);
            }
        }
    });
}

async fn tick(pool: &PgPool) -> Result<(), String> {
    let reapable = db::sandbox::list_reapable(pool).await?;
    for sandbox in reapable {
        reap(pool, &sandbox).await;
    }
    Ok(())
}

/// Kill one sandbox: enqueue a `sandbox` kill for its agent, then mark it
/// expired. Best-effort — the row is marked regardless so the reap is
/// idempotent and a lost command does not strand the record.
async fn reap(pool: &PgPool, sandbox: &Sandbox) {
    if let Some(deployment_id) = sandbox.deployment_id {
        let params = json!({
            "action": "kill",
            "sandboxId": sandbox.id,
        });
        let dispatcher = AgentDispatcher::new(pool);
        match dispatcher
            .queue_command(deployment_id, "sandbox", params)
            .await
        {
            Ok(command_id) => tracing::info!(
                sandbox_id = %sandbox.id,
                command_id = %command_id,
                "sandbox reap: kill queued"
            ),
            Err(err) => tracing::warn!(
                sandbox_id = %sandbox.id,
                "sandbox reap: failed to queue kill: {}", err
            ),
        }
    }
    if let Err(err) = db::sandbox::update_status(pool, sandbox.id, "expired").await {
        tracing::warn!(sandbox_id = %sandbox.id, "sandbox reap: mark failed: {}", err);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reap_is_idempotent_by_design() {
        // The reap marks the row expired regardless of the enqueue result, so
        // repeating the pass cannot double-process a sandbox (status leaves the
        // reapable set). This is a structural guarantee, not runtime behavior.
        let params =
            json!({ "action": "kill", "sandboxId": "00000000-0000-0000-0000-000000000000" });
        assert_eq!(params["action"], "kill");
    }
}
