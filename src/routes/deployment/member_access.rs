//! Which role a user holds on a deployment, for the User Service.
//!
//! Deployments of a shared project are recorded under the project owner, so
//! the User Service installation endpoints only recognise the owner. They ask
//! here before letting a project member operate the server. Roles live only in
//! Stacker (`project_member`), so this keeps them in one place.

use crate::db;
use crate::helpers::JsonResponse;
use crate::routes::marketplace::public::require_stacker_service_auth;
use crate::routes::project::access::{access_of, Access};
use actix_web::{get, web, HttpRequest, Responder, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Deserialize)]
pub struct MemberAccessQuery {
    pub user_id: String,
}

#[derive(Debug, Serialize)]
struct MemberAccess {
    /// "owner", "member", "viewer", or null when the user has no access or
    /// the deployment is unknown.
    role: Option<&'static str>,
}

#[tracing::instrument(name = "Deployment member access", skip_all)]
#[get("/{deployment_hash}/member-access")]
pub async fn member_access_handler(
    req: HttpRequest,
    path: web::Path<(String,)>,
    query: web::Query<MemberAccessQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    require_stacker_service_auth(&req)?;

    let deployment = db::deployment::fetch_by_deployment_hash(pg_pool.get_ref(), &path.0)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    let project = match deployment {
        Some(deployment) => db::project::fetch(pg_pool.get_ref(), deployment.project_id)
            .await
            .map_err(JsonResponse::internal_server_error)?,
        None => None,
    };
    let access = match project {
        Some(project) => access_of(pg_pool.get_ref(), &project, &query.user_id)
            .await
            .map_err(JsonResponse::internal_server_error)?,
        None => None,
    };
    let role = access.map(|access| match access {
        Access::Owner => "owner",
        Access::Member => "member",
        Access::Viewer => "viewer",
    });

    Ok(JsonResponse::build()
        .set_item(MemberAccess { role })
        .ok("OK"))
}
