//! Redeploy a saved project onto its existing server (WS-G G5).
//!
//! Stacker has no programmatic "redeploy": `deploy_project` expects the full
//! Deploy form that a client sends. This module rebuilds that form from the
//! project's saved state — the recorded request plus the saved server and
//! cloud rows — and pins `server.server_id`, so the deploy reuses the
//! existing server (deploy.rs:1720) and never provisions a new VM.
//!
//! Design: config/PLAN-github-webhooks.md (G5, 2026-11-24).

use crate::configuration::Settings;
use crate::connectors::{
    install_service::InstallServiceConnector, user_service::UserServiceConnector,
};
use crate::db;
use crate::forms;
use crate::helpers::{MqManager, VaultClient};
use crate::models;
use sqlx::PgPool;
use std::sync::Arc;

/// One auto-deploy attempt against a saved project.
pub struct RedeployAttempt {
    pub project_id: i32,
    pub deployment_id: Option<i32>,
    /// `queued` | `skipped` | `failed`
    pub outcome: String,
    pub reason: Option<String>,
}

/// Rebuild the Deploy form for a redeploy onto the project's saved server.
pub fn rebuild_deploy_form(
    project: &models::Project,
    server: &models::Server,
    cloud: Option<&models::Cloud>,
) -> forms::project::Deploy {
    let stack_code = project
        .request_json
        .get("custom")
        .and_then(|custom| custom.get("custom_stack_code"))
        .and_then(|value| value.as_str())
        .map(str::to_string);

    forms::project::Deploy {
        stack: forms::project::Stack {
            stack_code,
            ..Default::default()
        },
        // The point of the exercise: reuse the existing server row.
        server: forms::ServerForm {
            server_id: Some(server.id),
            ..Default::default()
        },
        cloud: forms::CloudForm {
            provider: cloud
                .map(|cloud| cloud.provider.clone())
                .unwrap_or_else(|| "own".to_string()),
            name: cloud.map(|cloud| cloud.name.clone()),
            ..Default::default()
        },
        initiator_email: Some("github-app".to_string()),
        ..Default::default()
    }
}

/// Redeploy a project from its saved state. Best-effort by design: every
/// outcome is reported, nothing throws.
pub async fn redeploy_saved_project(
    user: &models::User,
    project: &models::Project,
    pg_pool: &PgPool,
    mq_manager: &MqManager,
    settings: &Settings,
    user_service: &Arc<dyn UserServiceConnector>,
    install_service: &Arc<dyn InstallServiceConnector>,
    vault_client: &VaultClient,
) -> RedeployAttempt {
    let mut attempt = RedeployAttempt {
        project_id: project.id,
        deployment_id: None,
        outcome: "failed".to_string(),
        reason: None,
    };

    let servers = match db::server::fetch_by_project(pg_pool, project.id).await {
        Ok(servers) => servers,
        Err(error) => {
            attempt.reason = Some(error);
            return attempt;
        }
    };
    let Some(server) = servers.into_iter().max_by_key(|server| server.id) else {
        attempt.outcome = "skipped".to_string();
        attempt.reason = Some(
            "project has no saved server; deploy it once before enabling auto-deploy".to_string(),
        );
        return attempt;
    };

    let cloud = match server.cloud_id {
        Some(cloud_id) => db::cloud::fetch(pg_pool, cloud_id).await.ok().flatten(),
        None => None,
    };

    let form = rebuild_deploy_form(project, &server, cloud.as_ref());
    match crate::routes::project::deploy::deploy_project(
        user,
        project.clone(),
        form,
        pg_pool,
        mq_manager,
        settings,
        user_service,
        install_service,
        vault_client,
    )
    .await
    {
        Ok((project_id, deployment_id)) => {
            attempt.project_id = project_id;
            attempt.deployment_id = Some(deployment_id);
            attempt.outcome = "queued".to_string();
        }
        Err(error) => {
            attempt.reason = Some(error.to_string().chars().take(300).collect());
        }
    }
    attempt
}

/// Build the acting user for a webhook-triggered deploy: the connected repo's
/// owner. No user-service session exists in this context, so member checks
/// and marketplace access see a plain owner without an access token.
pub fn webhook_acting_user(user_id: &str) -> models::User {
    models::User {
        id: user_id.to_string(),
        first_name: String::new(),
        last_name: String::new(),
        email: String::new(),
        role: "group_user".to_string(),
        email_confirmed: true,
        mfa_verified: false,
        access_token: None,
    }
}
