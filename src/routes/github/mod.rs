//! GitHub App webhook receiver (WS-G G2).
//!
//! The route is casbin-anonymous: the caller is authenticated by the
//! `X-Hub-Signature-256` HMAC instead (precedent: payout webhook).
//! Signature verification, payload normalization and watch-path matching
//! live in the pure `github-events` crate.

use crate::connectors::{
    install_service::InstallServiceConnector, user_service::UserServiceConnector,
};
use crate::db;
use crate::helpers::{JsonResponse, MqManager, VaultClient};
use crate::services::VaultService;
use actix_web::{post, web, HttpRequest, Responder, Result};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

pub mod repos;

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
}

#[tracing::instrument(name = "GitHub webhook", skip_all)]
#[post("/webhook")]
pub async fn webhook_handler(
    request: HttpRequest,
    body: web::Bytes,
    pg_pool: web::Data<PgPool>,
    mq_manager: web::Data<MqManager>,
    sets: web::Data<crate::configuration::Settings>,
    user_service: web::Data<Arc<dyn UserServiceConnector>>,
    install_service: web::Data<Arc<dyn InstallServiceConnector>>,
    vault_client: web::Data<VaultClient>,
) -> Result<impl Responder> {
    let event_name = header(&request, "X-GitHub-Event")
        .ok_or_else(|| JsonResponse::bad_request("Missing X-GitHub-Event header"))?
        .to_string();
    let delivery_id = header(&request, "X-GitHub-Delivery")
        .ok_or_else(|| JsonResponse::bad_request("Missing X-GitHub-Delivery header"))?
        .to_string();
    let signature = header(&request, "X-Hub-Signature-256");

    // The GitHub App webhook secret lives in Vault only.
    let vault = VaultService::from_settings(&sets.vault)
        .map_err(|error| JsonResponse::internal_server_error(error.to_string()))?;
    let secret = vault
        .fetch_secret_value(&vault.github_webhook_secret_path())
        .await
        .map_err(|_| {
            JsonResponse::internal_server_error("GitHub webhook secret is not configured")
        })?;

    github_events::verify_signature(&secret, &body, signature)
        .map_err(|error| JsonResponse::unauthorized(error))?;

    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|error| JsonResponse::bad_request(format!("Invalid JSON payload: {}", error)))?;

    // Replay protection: a delivery id is processed at most once (G2).
    let is_new = db::github::record_delivery(pg_pool.get_ref(), &delivery_id, &event_name)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    if !is_new {
        return Ok(JsonResponse::build()
            .set_item(json!({ "received": true, "duplicate": true }))
            .ok("Duplicate delivery ignored"));
    }

    let event = github_events::parse_event(&event_name, &payload)
        .map_err(|error| JsonResponse::bad_request(error))?;

    // G5: matched repos are auto-deployed onto their saved servers below.
    let mut matched = Vec::new();
    let mut matched_rows = Vec::new();
    match &event {
        github_events::GitEvent::Push(push) => {
            let repos = db::github::list_push_repos(pg_pool.get_ref(), &push.repo, &push.branch)
                .await
                .map_err(JsonResponse::internal_server_error)?;
            for repo in repos {
                match github_events::matches_watch_paths(&repo.watch_paths, &push.changed_files) {
                    Ok(true) => {
                        matched.push(json!({
                            "id": repo.id,
                            "projectId": repo.project_id,
                            "repo": repo.repo,
                            "branch": repo.branch,
                        }));
                        matched_rows.push(repo);
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(
                            repo = %repo.repo,
                            "watch path patterns rejected, push not matched: {}", error
                        );
                    }
                }
            }
        }
        github_events::GitEvent::Installation(installation) => {
            db::github::upsert_installation(
                pg_pool.get_ref(),
                installation.installation_id,
                Some(&installation.account_login),
            )
            .await
            .map_err(JsonResponse::internal_server_error)?;
        }
        github_events::GitEvent::InstallationRepositories(update) => {
            db::github::upsert_installation(pg_pool.get_ref(), update.installation_id, None)
                .await
                .map_err(JsonResponse::internal_server_error)?;
        }
        _ => {}
    }

    // G5: auto-deploy each matched repo's project onto its saved server
    // (never provisions a new VM). Best-effort per repo, reported inline.
    let mut triggered = Vec::new();
    for repo in matched_rows {
        let Some(project_id) = repo.project_id else {
            continue;
        };
        let attempt = match db::project::fetch(pg_pool.get_ref(), project_id).await {
            Ok(Some(project)) => {
                let user = crate::services::redeploy::webhook_acting_user(&repo.user_id);
                crate::services::redeploy::redeploy_saved_project(
                    &user,
                    &project,
                    pg_pool.get_ref(),
                    mq_manager.get_ref(),
                    sets.get_ref(),
                    user_service.get_ref(),
                    install_service.get_ref(),
                    vault_client.get_ref(),
                )
                .await
            }
            Ok(None) => crate::services::redeploy::RedeployAttempt {
                project_id,
                deployment_id: None,
                outcome: "skipped".to_string(),
                reason: Some("project not found".to_string()),
            },
            Err(error) => crate::services::redeploy::RedeployAttempt {
                project_id,
                deployment_id: None,
                outcome: "failed".to_string(),
                reason: Some(error),
            },
        };
        triggered.push(json!({
            "projectId": attempt.project_id,
            "deploymentId": attempt.deployment_id,
            "outcome": attempt.outcome,
            "reason": attempt.reason,
        }));
    }

    Ok(JsonResponse::build()
        .set_item(json!({
            "received": true,
            "event": event_name,
            "matched": matched,
            "triggered": triggered,
        }))
        .ok("Webhook processed"))
}
