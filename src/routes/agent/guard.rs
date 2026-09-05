//! Ownership checks for `/v1/agent` routes.
//!
//! Casbin authorises these routes by path pattern only — e.g.
//! `('p','agent','/api/v1/agent/deployments/*','GET')`. The `*` matches any
//! `deployment_hash`, so route-level authorisation can establish *that* the
//! caller is an agent or a user, never *which* deployment they may see.
//! Without a per-request ownership check, one tenant's agent could read
//! another tenant's data by substituting a hash in the URL.
//!
//! These routes are reachable by two kinds of caller — the agent itself, and
//! a dashboard user (`group_user` / `group_admin` / `root` all carry the same
//! Casbin grants) — so the check has to cover both:
//!
//! * agent  → its own `deployment_hash` and no other;
//! * user   → a deployment they own, via [`resolve_owned_deployment_by_hash`].
//!
//! `wait.rs`, `report.rs` and `notifications.rs` already compared
//! `agent.deployment_hash` inline; `snapshot.rs` and `audit.rs` did not.

use crate::configuration::Settings;
use crate::helpers::JsonResponse;
use crate::models;
use crate::routes::legacy_installations::resolve_owned_deployment_by_hash;
use sqlx::PgPool;
use std::sync::Arc;

/// Authorise access to everything scoped by `deployment_hash`.
///
/// Returns `Ok(())` only when the caller demonstrably owns that deployment.
/// A caller carrying neither identity is rejected: reaching a handler without
/// one means the authentication middleware did not run for this route, and
/// failing closed is the only safe reading of that.
pub async fn authorize_deployment_access(
    pg_pool: &PgPool,
    settings: &Settings,
    deployment_hash: &str,
    agent: Option<&Arc<models::Agent>>,
    user: Option<&Arc<models::User>>,
) -> Result<(), actix_web::Error> {
    if let Some(agent) = agent {
        if agent.deployment_hash == deployment_hash {
            return Ok(());
        }
        // Deliberately "not found" rather than "forbidden": a distinguishable
        // 403 would confirm that some other tenant's deployment_hash exists.
        return Err(JsonResponse::<String>::not_found("Deployment not found"));
    }

    if let Some(user) = user {
        resolve_owned_deployment_by_hash(pg_pool, settings, user, deployment_hash).await?;
        return Ok(());
    }

    Err(JsonResponse::<String>::forbidden(
        "Not authorized for this deployment",
    ))
}

/// Authorise access to a project-scoped route.
///
/// An agent belongs to exactly one deployment, so its project is whatever that
/// deployment points at; a user must own the project outright.
pub async fn authorize_project_access(
    pg_pool: &PgPool,
    project_id: i32,
    agent: Option<&Arc<models::Agent>>,
    user: Option<&Arc<models::User>>,
) -> Result<(), actix_web::Error> {
    if let Some(agent) = agent {
        let deployment =
            crate::db::deployment::fetch_by_deployment_hash(pg_pool, &agent.deployment_hash)
                .await
                .map_err(JsonResponse::<String>::internal_server_error)?;

        let owns = deployment
            .as_ref()
            .is_some_and(|d| d.project_id == project_id);

        if owns {
            return Ok(());
        }
        return Err(JsonResponse::<String>::not_found("Project not found"));
    }

    if let Some(user) = user {
        let project = crate::db::project::fetch(pg_pool, project_id)
            .await
            .map_err(JsonResponse::<String>::internal_server_error)?;

        let owns = project.as_ref().is_some_and(|p| p.user_id == user.id);
        if owns {
            return Ok(());
        }
        return Err(JsonResponse::<String>::not_found("Project not found"));
    }

    Err(JsonResponse::<String>::forbidden(
        "Not authorized for this project",
    ))
}
