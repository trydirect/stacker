//! The project owner's saved clouds, for whoever deploys the project.
//!
//! A member deploys with the owner's saved cloud (the deployment runs as the
//! owner), so they need to pick one, but never see its credentials: only the
//! id, name and provider are returned.

use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use crate::routes::project::access::{self, Need};
use actix_web::{get, web, Responder, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct ProjectCloud {
    pub id: i32,
    pub name: String,
    pub provider: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<models::Cloud> for ProjectCloud {
    fn from(cloud: models::Cloud) -> Self {
        ProjectCloud {
            id: cloud.id,
            name: cloud.name,
            provider: cloud.provider,
            created_at: cloud.created_at,
            updated_at: cloud.updated_at,
        }
    }
}

#[tracing::instrument(name = "List the project owner's clouds", skip_all)]
#[get("/{id}/clouds")]
pub async fn list(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project, _) = access::require(pg_pool.get_ref(), path.0, &user.id, Need::Manage).await?;
    let clouds = db::cloud::fetch_by_user(pg_pool.get_ref(), &project.user_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .into_iter()
        .map(ProjectCloud::from)
        .collect::<Vec<_>>();
    Ok(JsonResponse::build().set_list(clouds).ok("OK"))
}
