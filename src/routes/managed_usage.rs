//! Internal endpoints the User Service calls for the Managed server of a
//! Quick Deploy (contract: shared-fixtures/api-contracts/managed-server-billing.json).
//!
//! Casbin admits them for `group_anonymous` (the caller carries no user
//! token); `require_internal_key` is the authorisation.

use actix_web::{post, web, HttpRequest, HttpResponse, Result};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;

use crate::db::managed_usage;
use crate::helpers::internal_key::require_internal_key;

/// 404 when the deployment has no usage record, 409 when it is in another state.
async fn not_changed(pool: &PgPool, deployment_hash: &str, action: &str) -> HttpResponse {
    match managed_usage::fetch_by_hash(pool, deployment_hash).await {
        Ok(None) => HttpResponse::NotFound().json(json!({
            "error": "no usage record for the deployment",
            "deployment_hash": deployment_hash,
        })),
        Ok(Some(row)) => HttpResponse::Conflict().json(json!({
            "error": format!("cannot {action} usage in status {}", row.status),
            "deployment_hash": deployment_hash,
            "status": row.status,
        })),
        Err(err) => HttpResponse::InternalServerError().json(json!({ "error": err })),
    }
}

/// `POST /api/internal/managed-usage/{deployment_hash}/start`: the deploy succeeded.
#[post("/{deployment_hash}/start")]
pub async fn start(
    req: HttpRequest,
    path: web::Path<String>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    require_internal_key(&req)?;
    let deployment_hash = path.into_inner();
    Ok(match managed_usage::start(&pool, &deployment_hash).await {
        Ok(Some(row)) => HttpResponse::Ok().json(json!({
            "deployment_hash": row.deployment_hash,
            "status": row.status,
        })),
        Ok(None) => not_changed(&pool, &deployment_hash, "start").await,
        Err(err) => HttpResponse::InternalServerError().json(json!({ "error": err })),
    })
}

#[derive(Debug, Default, Deserialize)]
pub struct FailBody {
    #[serde(default)]
    pub error: Option<String>,
}

/// `POST /api/internal/managed-usage/{deployment_hash}/fail`: the deploy failed.
#[post("/{deployment_hash}/fail")]
pub async fn fail(
    req: HttpRequest,
    path: web::Path<String>,
    body: Option<web::Json<FailBody>>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    require_internal_key(&req)?;
    let deployment_hash = path.into_inner();
    if let Some(error) = body.as_ref().and_then(|b| b.error.as_deref()) {
        tracing::info!(deployment_hash = %deployment_hash, error = %error, "quick deploy failed");
    }
    Ok(
        match managed_usage::close(&pool, &deployment_hash, "deploy_failed").await {
            Ok(Some(row)) => HttpResponse::Ok().json(json!({
                "deployment_hash": row.deployment_hash,
                "status": row.status,
                "outcome": row.outcome,
            })),
            Ok(None) => not_changed(&pool, &deployment_hash, "close").await,
            Err(err) => HttpResponse::InternalServerError().json(json!({ "error": err })),
        },
    )
}

#[derive(Debug, Deserialize)]
pub struct SuspendBody {
    #[serde(default)]
    pub user_id: Option<String>,
}

/// `POST /api/internal/managed-usage/suspend`: the user's invoice is unpaid
/// after Stripe's retries; every active server of the user stops being billed.
#[post("/suspend")]
pub async fn suspend(
    req: HttpRequest,
    body: web::Json<SuspendBody>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    require_internal_key(&req)?;
    let Some(user_id) = body.user_id.as_deref().filter(|id| !id.trim().is_empty()) else {
        return Ok(HttpResponse::BadRequest().json(json!({ "error": "user_id is required" })));
    };
    Ok(
        match managed_usage::suspend_for_user(&pool, user_id).await {
            Ok(rows) => HttpResponse::Ok().json(json!({
                "suspended": rows.iter().map(|r| r.deployment_hash.clone()).collect::<Vec<_>>(),
            })),
            Err(err) => HttpResponse::InternalServerError().json(json!({ "error": err })),
        },
    )
}
