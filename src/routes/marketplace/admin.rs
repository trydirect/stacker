use crate::db;
use crate::connectors::user_service::UserServiceConnector;
use crate::connectors::{MarketplaceWebhookSender, WebhookSenderConfig};
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, post, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use uuid;
use tracing::Instrument;

#[tracing::instrument(name = "List submitted templates (admin)")]
#[get("")]
pub async fn list_submitted_handler(
    _admin: web::ReqData<Arc<models::User>>, // role enforced by Casbin
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::marketplace::admin_list_submitted(pg_pool.get_ref())
        .await
        .map_err(|err| JsonResponse::<Vec<models::StackTemplate>>::build().internal_server_error(err))
        .map(|templates| JsonResponse::build().set_list(templates).ok("OK"))
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
    
    let updated = db::marketplace::admin_decide(pg_pool.get_ref(), &id, &admin.id, "approved", req.reason.as_deref())
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
                let span = tracing::info_span!("send_approval_webhook", template_id = %template_clone.id);
                
                if let Err(e) = sender
                    .send_template_approved(&template_clone, &template_clone.creator_user_id)
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
    
    let updated = db::marketplace::admin_decide(pg_pool.get_ref(), &id, &admin.id, "rejected", req.reason.as_deref())
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
                let span = tracing::info_span!("send_rejection_webhook", template_id = %template_id);
                
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