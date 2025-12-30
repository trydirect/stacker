use crate::db;
use crate::connectors::user_service::UserServiceConnector;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, post, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use uuid;

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

    if updated {
        Ok(JsonResponse::<serde_json::Value>::build().ok("Approved"))
    } else {
        Err(JsonResponse::<serde_json::Value>::build().bad_request("Not updated"))
    }
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

    if updated {
        Ok(JsonResponse::<serde_json::Value>::build().ok("Rejected"))
    } else {
        Err(JsonResponse::<serde_json::Value>::build().bad_request("Not updated"))
    }
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