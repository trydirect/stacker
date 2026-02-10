use crate::connectors::user_service::UserServiceConnector;
use crate::connectors::{MarketplaceWebhookSender, WebhookSenderConfig};
use crate::db;
use crate::helpers::security_validator;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, post, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;
use uuid;

#[tracing::instrument(name = "List submitted templates (admin)")]
#[get("")]
pub async fn list_submitted_handler(
    _admin: web::ReqData<Arc<models::User>>, // role enforced by Casbin
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::marketplace::admin_list_submitted(pg_pool.get_ref())
        .await
        .map_err(|err| {
            JsonResponse::<Vec<models::StackTemplate>>::build().internal_server_error(err)
        })
        .map(|templates| JsonResponse::build().set_list(templates).ok("OK"))
}

#[tracing::instrument(name = "Get template detail (admin)")]
#[get("/{id}")]
pub async fn detail_handler(
    _admin: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<web::Json<crate::helpers::json::JsonResponse<serde_json::Value>>> {
    let id = uuid::Uuid::parse_str(&path.into_inner().0)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;

    let template = db::marketplace::get_by_id(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?
        .ok_or_else(|| {
            JsonResponse::<serde_json::Value>::build().not_found("Template not found")
        })?;

    let versions = db::marketplace::list_versions_by_template(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?;

    let reviews = db::marketplace::list_reviews_by_template(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?;

    let detail = serde_json::json!({
        "template": template,
        "versions": versions,
        "reviews": reviews,
    });

    Ok(JsonResponse::<serde_json::Value>::build()
        .set_item(detail)
        .ok("OK"))
}

#[derive(serde::Deserialize, Debug)]
pub struct AdminDecisionRequest {
    pub decision: String, // approved|rejected|needs_changes
    pub reason: Option<String>,
}

#[tracing::instrument(name = "Approve template (admin)")]
#[post("/{id}/approve")]
pub async fn approve_handler(
    admin: web::ReqData<Arc<models::User>>, // role enforced by Casbin
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
    body: web::Json<AdminDecisionRequest>,
) -> Result<web::Json<crate::helpers::json::JsonResponse<serde_json::Value>>> {
    let id = uuid::Uuid::parse_str(&path.into_inner().0)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;
    let req = body.into_inner();

    let updated = db::marketplace::admin_decide(
        pg_pool.get_ref(),
        &id,
        &admin.id,
        "approved",
        req.reason.as_deref(),
    )
    .await
    .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?;

    if !updated {
        return Err(JsonResponse::<serde_json::Value>::build().bad_request("Not updated"));
    }

    // Fetch template details for webhook
    let template = db::marketplace::get_by_id(pg_pool.get_ref(), id)
        .await
        .map_err(|err| {
            tracing::error!("Failed to fetch template for webhook: {:?}", err);
            JsonResponse::<serde_json::Value>::build().internal_server_error(err)
        })?
        .ok_or_else(|| {
            JsonResponse::<serde_json::Value>::build().not_found("Template not found")
        })?;

    // Send webhook asynchronously (non-blocking)
    // Don't fail the approval if webhook send fails - template is already approved
    let template_clone = template.clone();
    tokio::spawn(async move {
        match WebhookSenderConfig::from_env() {
            Ok(config) => {
                let sender = MarketplaceWebhookSender::new(config);
                let span =
                    tracing::info_span!("send_approval_webhook", template_id = %template_clone.id);

                if let Err(e) = sender
                    .send_template_approved(
                        &template_clone,
                        &template_clone.creator_user_id,
                        template_clone.category_code.clone(),
                    )
                    .instrument(span)
                    .await
                {
                    tracing::warn!("Failed to send template approval webhook: {:?}", e);
                    // Log but don't block - approval already persisted
                }
            }
            Err(e) => {
                tracing::warn!("Webhook sender config not available: {}", e);
                // Gracefully handle missing config
            }
        }
    });

    Ok(JsonResponse::<serde_json::Value>::build().ok("Approved"))
}

#[tracing::instrument(name = "Reject template (admin)")]
#[post("/{id}/reject")]
pub async fn reject_handler(
    admin: web::ReqData<Arc<models::User>>, // role enforced by Casbin
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
    body: web::Json<AdminDecisionRequest>,
) -> Result<web::Json<crate::helpers::json::JsonResponse<serde_json::Value>>> {
    let id = uuid::Uuid::parse_str(&path.into_inner().0)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;
    let req = body.into_inner();

    let updated = db::marketplace::admin_decide(
        pg_pool.get_ref(),
        &id,
        &admin.id,
        "rejected",
        req.reason.as_deref(),
    )
    .await
    .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?;

    if !updated {
        return Err(JsonResponse::<serde_json::Value>::build().bad_request("Not updated"));
    }

    // Send webhook asynchronously (non-blocking)
    // Don't fail the rejection if webhook send fails - template is already rejected
    let template_id = id.to_string();
    tokio::spawn(async move {
        match WebhookSenderConfig::from_env() {
            Ok(config) => {
                let sender = MarketplaceWebhookSender::new(config);
                let span =
                    tracing::info_span!("send_rejection_webhook", template_id = %template_id);

                if let Err(e) = sender
                    .send_template_rejected(&template_id)
                    .instrument(span)
                    .await
                {
                    tracing::warn!("Failed to send template rejection webhook: {:?}", e);
                    // Log but don't block - rejection already persisted
                }
            }
            Err(e) => {
                tracing::warn!("Webhook sender config not available: {}", e);
                // Gracefully handle missing config
            }
        }
    });

    Ok(JsonResponse::<serde_json::Value>::build().ok("Rejected"))
}

#[tracing::instrument(name = "Security scan template (admin)")]
#[post("/{id}/security-scan")]
pub async fn security_scan_handler(
    admin: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<web::Json<crate::helpers::json::JsonResponse<serde_json::Value>>> {
    let id = uuid::Uuid::parse_str(&path.into_inner().0)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;

    // Fetch template
    let template = db::marketplace::get_by_id(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?
        .ok_or_else(|| {
            JsonResponse::<serde_json::Value>::build().not_found("Template not found")
        })?;

    // Fetch versions to get latest stack_definition
    let versions = db::marketplace::list_versions_by_template(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?;

    let latest = versions
        .iter()
        .find(|v| v.is_latest == Some(true))
        .or_else(|| versions.first())
        .ok_or_else(|| {
            JsonResponse::<serde_json::Value>::build()
                .bad_request("No versions found for this template")
        })?;

    // Run automated security validation
    let report = security_validator::validate_stack_security(&latest.stack_definition);

    // Save scan result as a review record
    let review = db::marketplace::save_security_scan(
        pg_pool.get_ref(),
        &id,
        &admin.id,
        report.to_checklist_json(),
    )
    .await
    .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?;

    let result = serde_json::json!({
        "template_id": template.id,
        "template_name": template.name,
        "version": latest.version,
        "review_id": review.id,
        "overall_passed": report.overall_passed,
        "risk_score": report.risk_score,
        "no_secrets": report.no_secrets,
        "no_hardcoded_creds": report.no_hardcoded_creds,
        "valid_docker_syntax": report.valid_docker_syntax,
        "no_malicious_code": report.no_malicious_code,
        "recommendations": report.recommendations,
    });

    Ok(JsonResponse::<serde_json::Value>::build()
        .set_item(result)
        .ok("Security scan completed"))
}

#[tracing::instrument(name = "List available plans from User Service", skip(user_service))]
#[get("/plans")]
pub async fn list_plans_handler(
    _admin: web::ReqData<Arc<models::User>>, // role enforced by Casbin
    user_service: web::Data<Arc<dyn UserServiceConnector>>,
) -> Result<impl Responder> {
    user_service
        .list_available_plans()
        .await
        .map_err(|err| {
            tracing::error!("Failed to fetch available plans: {:?}", err);
            JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Failed to fetch available plans from User Service")
        })
        .map(|plans| {
            // Convert PlanDefinition to JSON for response
            let plan_json: Vec<serde_json::Value> = plans
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "name": p.name,
                        "description": p.description,
                        "tier": p.tier,
                        "features": p.features
                    })
                })
                .collect();
            JsonResponse::build().set_list(plan_json).ok("OK")
        })
}
