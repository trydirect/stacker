//! Who may do what on a project.
//!
//! A project has one owner and any number of members (`project_member`). A
//! member with role "member" works on the stack; a "viewer" only looks.
//! Someone with no access gets the same 404 as for a missing project, so a
//! project id reveals nothing. Someone with access who asks for more than
//! their role allows gets 403.

use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Owner,
    Member,
    Viewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Read the stack: payload, compose, apps, env. Owner and member. A
    /// viewer sees only the shared list and deployments (deployment routes
    /// check membership themselves).
    View,
    /// Change the stack and deploy it.
    Manage,
    /// Secrets, deletion, protection, members and invitations.
    Owner,
}

impl Access {
    pub fn allows(self, need: Need) -> bool {
        match (self, need) {
            (Access::Owner, _) => true,
            (Access::Member, Need::View | Need::Manage) => true,
            _ => false,
        }
    }
}

/// The caller's access to a project, or None when they have none.
pub async fn access_of(
    pool: &PgPool,
    project: &models::Project,
    user_id: &str,
) -> Result<Option<Access>, String> {
    if project.user_id == user_id {
        return Ok(Some(Access::Owner));
    }
    let member = db::project_member::fetch(pool, project.id, user_id).await?;
    Ok(member.and_then(|m| match m.role.as_str() {
        "member" => Some(Access::Member),
        "viewer" => Some(Access::Viewer),
        _ => None,
    }))
}

/// Load a project the caller may act on as `need` requires.
pub async fn require(
    pool: &PgPool,
    project_id: i32,
    user_id: &str,
    need: Need,
) -> Result<(models::Project, Access), actix_web::Error> {
    let project = db::project::fetch(pool, project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("not found"))?;

    let access = access_of(pool, &project, user_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("not found"))?;

    if !access.allows(need) {
        return Err(JsonResponse::forbidden("forbidden"));
    }
    Ok((project, access))
}

/// Check access to a project the handler has already loaded.
///
/// `not_found` is the message the handler answers for a missing project, so
/// someone with no access gets exactly the same response as for a project
/// that does not exist.
pub async fn check(
    pool: &PgPool,
    project: &models::Project,
    user_id: &str,
    need: Need,
    not_found: &str,
) -> Result<Access, actix_web::Error> {
    let access = access_of(pool, project, user_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found(not_found))?;
    if !access.allows(need) {
        return Err(JsonResponse::forbidden("forbidden"));
    }
    Ok(access)
}

/// Resolve a deployment the caller may act on as `need` requires.
///
/// The owner of a native deployment always may. A member of the project the
/// deployment belongs to may when their role allows it; a viewer gets 403.
/// Anyone else, and every legacy User Service installation that is not the
/// caller's, gets the owner-only lookup's 404.
pub async fn resolve_team_deployment(
    pool: &PgPool,
    settings: &crate::configuration::Settings,
    user: &models::User,
    deployment_hash: &str,
    need: Need,
) -> Result<crate::routes::legacy_installations::OwnedDeployment, actix_web::Error> {
    use crate::routes::legacy_installations::{resolve_owned_deployment_by_hash, OwnedDeployment};

    let deployment = db::deployment::fetch_by_deployment_hash(pool, deployment_hash)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    if let Some(deployment) = deployment {
        if deployment.user_id.as_deref() != Some(user.id.as_str()) {
            let project = db::project::fetch(pool, deployment.project_id)
                .await
                .map_err(JsonResponse::internal_server_error)?;
            let access = match project {
                Some(project) => access_of(pool, &project, &user.id)
                    .await
                    .map_err(JsonResponse::internal_server_error)?,
                None => None,
            };
            return match access {
                Some(access) if access.allows(need) => Ok(OwnedDeployment::Native(deployment)),
                Some(_) => Err(JsonResponse::forbidden("forbidden")),
                None => Err(JsonResponse::not_found("Deployment not found")),
            };
        }
    }
    resolve_owned_deployment_by_hash(pool, settings, user, deployment_hash).await
}

/// Who a deployment runs as.
///
/// A member deploys on the owner's behalf: the owner's saved clouds, servers,
/// SSH keys and deployment quota are used, and everything is recorded under
/// the owner. The member's own token stays for calls made on their behalf.
/// The owner's email comes from the User Service, because the install
/// payload carries id and email together; if it cannot be found, the deploy
/// is refused rather than sent with a mismatched address.
pub async fn acting_user(
    project: &models::Project,
    user: &models::User,
    access: Access,
    user_service_url: &str,
) -> Result<models::User, actix_web::Error> {
    if access == Access::Owner {
        return Ok(user.clone());
    }
    let email = owner_email(user_service_url, &project.user_id)
        .await
        .map_err(|err| {
            tracing::warn!("Cannot deploy as the project owner: {}", err);
            actix_web::error::ErrorBadGateway(
                JsonResponse::<()>::build()
                    .set_msg("owner_unavailable")
                    .to_string(),
            )
        })?;
    Ok(models::User {
        id: project.user_id.clone(),
        first_name: String::new(),
        last_name: String::new(),
        email,
        role: user.role.clone(),
        email_confirmed: true,
        mfa_verified: user.mfa_verified,
        access_token: user.access_token.clone(),
    })
}

/// The member's address when a member deploys as the owner, so the deployment
/// reports reach them as well as the owner.
pub fn initiator_email(user: &models::User, access: Access) -> Option<String> {
    (access != Access::Owner).then(|| user.email.clone())
}

/// The owner's address from the User Service internal user lookup.
async fn owner_email(user_service_url: &str, owner_id: &str) -> Result<String, String> {
    let key = std::env::var("INTERNAL_SERVICES_ACCESS_KEY").unwrap_or_default();
    if key.is_empty() {
        return Err("INTERNAL_SERVICES_ACCESS_KEY is not set".to_string());
    }
    let url = format!(
        "{}/api/internal/users/{}",
        user_service_url.trim_end_matches('/'),
        urlencoding::encode(owner_id)
    );
    let response = reqwest::Client::new()
        .get(&url)
        .header("X-Internal-Service-Key", key)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|err| format!("owner lookup failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("owner lookup answered {}", response.status()));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("owner lookup returned bad JSON: {err}"))?;
    body.get("email")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "owner lookup returned no email".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_member_deploy_has_a_separate_initiator() {
        let member = models::User {
            id: "member-1".to_string(),
            first_name: String::new(),
            last_name: String::new(),
            email: "member@example.com".to_string(),
            role: "group_user".to_string(),
            email_confirmed: true,
            mfa_verified: false,
            access_token: None,
        };
        assert_eq!(
            initiator_email(&member, Access::Member).as_deref(),
            Some("member@example.com")
        );
        assert_eq!(initiator_email(&member, Access::Owner), None);
    }

    #[test]
    fn roles_allow_what_the_team_decisions_say() {
        use Access::*;
        use Need::View as V;
        assert!(Owner.allows(V) && Owner.allows(Need::Manage) && Owner.allows(Need::Owner));
        assert!(Member.allows(V) && Member.allows(Need::Manage));
        assert!(!Member.allows(Need::Owner));
        assert!(!Viewer.allows(V) && !Viewer.allows(Need::Manage) && !Viewer.allows(Need::Owner));
    }
}
