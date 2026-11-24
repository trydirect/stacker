//! GitHub App webhook receiver (WS-G G2).
//!
//! The route is casbin-anonymous: the caller is authenticated by the
//! `X-Hub-Signature-256` HMAC instead (precedent: payout webhook).
//! Signature verification, payload normalization and watch-path matching
//! live in the pure `github-events` crate.

use crate::db;
use crate::helpers::JsonResponse;
use crate::services::VaultService;
use actix_web::{post, web, HttpRequest, Responder, Result};
use serde_json::json;
use sqlx::PgPool;

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
    settings: web::Data<crate::configuration::Settings>,
) -> Result<impl Responder> {
    let event_name = header(&request, "X-GitHub-Event")
        .ok_or_else(|| JsonResponse::bad_request("Missing X-GitHub-Event header"))?
        .to_string();
    let delivery_id = header(&request, "X-GitHub-Delivery")
        .ok_or_else(|| JsonResponse::bad_request("Missing X-GitHub-Delivery header"))?
        .to_string();
    let signature = header(&request, "X-Hub-Signature-256");

    // The GitHub App webhook secret lives in Vault only.
    let vault = VaultService::from_settings(&settings.vault)
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

    // G5 seam: matched repos are where auto-deploy will be triggered; the
    // deploy plan wiring lands with G5.
    let mut matched = Vec::new();
    if let github_events::GitEvent::Push(push) = &event {
        let repos = db::github::list_push_repos(pg_pool.get_ref(), &push.repo, &push.branch)
            .await
            .map_err(JsonResponse::internal_server_error)?;
        for repo in repos {
            match github_events::matches_watch_paths(&repo.watch_paths, &push.changed_files) {
                Ok(true) => matched.push(json!({
                    "id": repo.id,
                    "projectId": repo.project_id,
                    "repo": repo.repo,
                    "branch": repo.branch,
                })),
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

    Ok(JsonResponse::build()
        .set_item(json!({
            "received": true,
            "event": event_name,
            "matched": matched,
        }))
        .ok("Webhook processed"))
}
