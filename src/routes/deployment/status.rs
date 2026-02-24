use actix_web::{get, web, Responder, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;

use crate::{db, helpers::JsonResponse, models};

/// Public-facing deployment status response (hides internal metadata).
#[derive(Debug, Clone, Serialize, Default)]
pub struct DeploymentStatusResponse {
    pub id: i32,
    pub project_id: i32,
    pub deployment_hash: String,
    pub status: String,
    /// Human-readable status/error message from the deployment pipeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<models::Deployment> for DeploymentStatusResponse {
    fn from(d: models::Deployment) -> Self {
        let status_message = d
            .metadata
            .get("status_message")
            .and_then(|v| v.as_str())
            .map(String::from);

        Self {
            id: d.id,
            project_id: d.project_id,
            deployment_hash: d.deployment_hash,
            status: d.status,
            status_message,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

/// `GET /api/v1/deployments/{id}`
///
/// Fetch deployment status by deployment ID.
/// Requires authentication (inherited from the `/api` scope middleware).
#[tracing::instrument(name = "Get deployment status by ID", skip(pg_pool))]
#[get("/{id}")]
pub async fn status_handler(
    path: web::Path<i32>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let deployment_id = path.into_inner();

    let deployment = db::deployment::fetch(pg_pool.get_ref(), deployment_id)
        .await
        .map_err(|err| JsonResponse::<DeploymentStatusResponse>::build().internal_server_error(err))?;

    match deployment {
        Some(d) => {
            // Verify the deployment belongs to the requesting user
            if d.user_id.as_deref() != Some(&user.id) {
                return Err(JsonResponse::<DeploymentStatusResponse>::build()
                    .not_found("Deployment not found"));
            }
            let resp: DeploymentStatusResponse = d.into();
            Ok(JsonResponse::build()
                .set_item(resp)
                .ok("Deployment status fetched"))
        }
        None => Err(JsonResponse::<DeploymentStatusResponse>::build()
            .not_found("Deployment not found")),
    }
}

/// `GET /api/v1/deployments/project/{project_id}`
///
/// Fetch the latest deployment status for a project.
/// Returns the most recent (non-deleted) deployment.
#[tracing::instrument(name = "Get deployment status by project ID", skip(pg_pool))]
#[get("/project/{project_id}")]
pub async fn status_by_project_handler(
    path: web::Path<i32>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let project_id = path.into_inner();

    let deployment = db::deployment::fetch_by_project_id(pg_pool.get_ref(), project_id)
        .await
        .map_err(|err| JsonResponse::<DeploymentStatusResponse>::build().internal_server_error(err))?;

    match deployment {
        Some(d) => {
            if d.user_id.as_deref() != Some(&user.id) {
                return Err(JsonResponse::<DeploymentStatusResponse>::build()
                    .not_found("No deployment found for this project"));
            }
            let resp: DeploymentStatusResponse = d.into();
            Ok(JsonResponse::build()
                .set_item(resp)
                .ok("Deployment status fetched"))
        }
        None => Err(JsonResponse::<DeploymentStatusResponse>::build()
            .not_found("No deployment found for this project")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_to_status_response() {
        let d = models::Deployment::new(
            42,
            Some("user123".to_string()),
            "deployment_abc".to_string(),
            "in_progress".to_string(),
            serde_json::json!({}),
        );
        let resp: DeploymentStatusResponse = d.into();
        assert_eq!(resp.project_id, 42);
        assert_eq!(resp.deployment_hash, "deployment_abc");
        assert_eq!(resp.status, "in_progress");
    }
}
