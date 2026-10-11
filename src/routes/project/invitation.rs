//! Team seats: inviting people to a project by email.
//!
//! A project has `PROJECT_SEATS` seats: the owner, every member, and every
//! pending invitation that has not expired. Only an owner on the Team plan may
//! fill a seat, by invitation or by adding a member directly. The plan is
//! checked when inviting, not when accepting, so members keep their access if
//! the owner later leaves Team.

use crate::configuration::Settings;
use crate::connectors::user_service::UserServiceClient;
use crate::db;
use crate::helpers::{agent_token, JsonResponse, MqManager};
use crate::models;
use crate::routes::project::access::{self, Need};
use actix_web::{delete, get, post, web, HttpResponse, Responder, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub const PROJECT_SEATS: i64 = 5;
pub const INVITATION_VALID_DAYS: i32 = 7;
pub const INVITABLE_ROLES: &[&str] = &["member", "viewer"];
const EMAIL_ROUTING_KEY: &str = "notify.start.email.project_invitation.all";
const EMAIL_PUBLISH_TIMEOUT: Duration = Duration::from_secs(5);

/// Team plan codes start with "plan-individual-". Free, Starter and Pro
/// cannot share projects.
pub fn can_share_projects(plan_code: &str) -> bool {
    plan_code.starts_with("plan-individual-")
}

/// The caller's current plan code, from the User Service with their own token.
async fn plan_code(settings: &Settings, user: &models::User) -> Result<Option<String>, String> {
    let token = user
        .access_token
        .as_deref()
        .ok_or_else(|| "no user token to check the plan with".to_string())?;
    UserServiceClient::new_public(&settings.user_service_url)
        .get_subscription_plan(token)
        .await
        .map(|plan| plan.code)
        .map_err(|err| err.to_string())
}

fn service_unavailable(msg: &str) -> actix_web::Error {
    actix_web::error::ErrorServiceUnavailable(JsonResponse::<()>::build().set_msg(msg).to_string())
}

/// Refuse unless the caller is on the Team plan. An unknown plan is refused
/// too: a User Service outage must not open sharing to everyone.
pub(crate) async fn require_team_plan(
    settings: &Settings,
    user: &models::User,
) -> Result<(), actix_web::Error> {
    match plan_code(settings, user).await {
        Ok(Some(code)) if can_share_projects(&code) => Ok(()),
        Ok(_) => Err(JsonResponse::forbidden("plan_required")),
        Err(err) => {
            tracing::warn!("Plan check failed: {}", err);
            Err(service_unavailable("plan_check_unavailable"))
        }
    }
}

/// Take the project's seat lock, drop expired invitations, and refuse when
/// every seat is taken. Call inside the transaction that fills the seat.
pub(crate) async fn reserve_seat(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project_id: i32,
) -> Result<(), actix_web::Error> {
    db::project_invitation::lock_project_seats(&mut **tx, project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    db::project_invitation::expire_stale(&mut **tx, project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    let taken = db::project_invitation::seats_taken(&mut **tx, project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    if taken >= PROJECT_SEATS {
        return Err(JsonResponse::<()>::build().conflict("seat_limit"));
    }
    Ok(())
}

fn frontend_url() -> String {
    std::env::var("STACKER_FRONTEND_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| "https://try.direct".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn invite_link(token: &str) -> String {
    format!("{}/invitations/{}", frontend_url(), token)
}

fn normalize_email(raw: &str) -> Option<String> {
    let email = raw.trim().to_lowercase();
    let (local, domain) = email.split_once('@')?;
    let valid = !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(char::is_whitespace)
        && email.len() <= 254;
    valid.then_some(email)
}

/// "colleague@try.direct" -> "c********@try.direct": enough to recognise
/// your own address, not enough to learn someone else's from a link.
fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => {
            let first: String = local.chars().take(1).collect();
            let stars = "*".repeat(local.chars().count().saturating_sub(1).max(3));
            format!("{first}{stars}@{domain}")
        }
        None => "***".to_string(),
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct InvitationEmail {
    to: String,
    subject: String,
    message: String,
    html: Option<String>,
}

pub(crate) fn invitation_email(
    to: &str,
    project_name: &str,
    invited_by: &str,
    link: &str,
) -> InvitationEmail {
    InvitationEmail {
        to: to.to_string(),
        subject: format!("{invited_by} invited you to {project_name} on TryDirect"),
        message: format!(
            "{invited_by} invited you to work on the project \"{project_name}\" on TryDirect.\n\n\
             Accept the invitation: {link}\n\n\
             The link is valid for {INVITATION_VALID_DAYS} days. If you do not have an account \
             yet, you can sign up from the link with this email address."
        ),
        html: Some(format!(
            r#"<p>{invited_by} invited you to work on the project <strong>{project_name}</strong> on TryDirect.</p>
<p><a href="{link}" style="display:inline-block;padding:12px 24px;background:#1976d2;color:#fff;text-decoration:none;border-radius:4px">Accept invitation</a></p>
<p>The link is valid for {INVITATION_VALID_DAYS} days. If you do not have an account yet, you can sign up from the link with this email address.</p>"#,
            invited_by = html_escape(invited_by),
            project_name = html_escape(project_name),
            link = html_escape(link),
        )),
    }
}

fn html_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Publish the invitation email. The invitation stands either way: the owner
/// gets the link back and can pass it on if the email does not arrive.
async fn send_invitation_email(mq: &MqManager, email: &InvitationEmail) -> bool {
    let publish = mq.publish("notify".to_string(), EMAIL_ROUTING_KEY.to_string(), email);
    match tokio::time::timeout(EMAIL_PUBLISH_TIMEOUT, publish).await {
        Ok(Ok(_)) => true,
        Ok(Err(err)) => {
            tracing::warn!("Invitation email not published: {}", err);
            false
        }
        Err(_) => {
            tracing::warn!("Invitation email not published: timed out");
            false
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateInvitationRequest {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Serialize)]
struct CreatedInvitation {
    id: Uuid,
    email: String,
    role: String,
    status: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    invite_link: String,
    email_sent: bool,
}

#[tracing::instrument(name = "Invite to project", skip_all)]
#[post("/{id}/invitations")]
pub async fn create(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    payload: web::Json<CreateInvitationRequest>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    mq_manager: web::Data<MqManager>,
) -> Result<impl Responder> {
    let (project, _) = access::require(pg_pool.get_ref(), path.0, &user.id, Need::Owner).await?;

    let email = normalize_email(&payload.email)
        .ok_or_else(|| JsonResponse::bad_request("invalid_email"))?;
    if !INVITABLE_ROLES.contains(&payload.role.as_str()) {
        return Err(JsonResponse::bad_request("invalid_role"));
    }
    require_team_plan(settings.get_ref(), &user).await?;
    if email == user.email.trim().to_lowercase() {
        return Err(JsonResponse::<()>::build().conflict("cannot_invite_self"));
    }

    let token = agent_token::generate();
    let mut tx = pg_pool
        .begin()
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;
    reserve_seat(&mut tx, project.id).await?;
    let already = db::project_invitation::live_pending_exists(&mut *tx, project.id, &email)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    if already {
        return Err(JsonResponse::<()>::build().conflict("already_invited"));
    }
    let invitation = db::project_invitation::insert(
        &mut *tx,
        db::project_invitation::NewInvitation {
            project_id: project.id,
            email: &email,
            role: &payload.role,
            token_hash: &agent_token::hash(&token),
            invited_by: &user.id,
            invited_by_email: &user.email,
            valid_for_days: INVITATION_VALID_DAYS,
        },
    )
    .await
    .map_err(JsonResponse::internal_server_error)?;
    tx.commit()
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;

    let link = invite_link(&token);
    let email_sent = send_invitation_email(
        mq_manager.get_ref(),
        &invitation_email(&email, &project.name, &user.email, &link),
    )
    .await;

    Ok(JsonResponse::build()
        .set_item(CreatedInvitation {
            id: invitation.id,
            email: invitation.email,
            role: invitation.role,
            status: invitation.status,
            expires_at: invitation.expires_at,
            invite_link: link,
            email_sent,
        })
        .created("OK"))
}

#[tracing::instrument(name = "List project invitations", skip_all)]
#[get("/{id}/invitations")]
pub async fn list(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project, _) = access::require(pg_pool.get_ref(), path.0, &user.id, Need::Owner).await?;
    let invitations = db::project_invitation::list_live_pending(pg_pool.get_ref(), project.id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    Ok(JsonResponse::build().set_list(invitations).ok("OK"))
}

#[tracing::instrument(name = "Revoke project invitation", skip_all)]
#[delete("/{id}/invitations/{invitation_id}")]
pub async fn revoke(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, String)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project_id, invitation_id) = path.into_inner();
    let (project, _) =
        access::require(pg_pool.get_ref(), project_id, &user.id, Need::Owner).await?;
    let invitation_id =
        Uuid::parse_str(&invitation_id).map_err(|_| JsonResponse::not_found("not found"))?;
    let revoked = db::project_invitation::revoke(pg_pool.get_ref(), project.id, invitation_id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    if !revoked {
        return Err(JsonResponse::not_found("not found"));
    }
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Debug, Serialize)]
struct Seats {
    seats_total: i64,
    seats_used: i64,
    seats_free: i64,
    can_invite: bool,
}

#[tracing::instrument(name = "Project seats", skip_all)]
#[get("/{id}/seats")]
pub async fn seats(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
) -> Result<impl Responder> {
    let (project, _) = access::require(pg_pool.get_ref(), path.0, &user.id, Need::Owner).await?;
    let mut tx = pg_pool
        .begin()
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;
    db::project_invitation::expire_stale(&mut *tx, project.id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    let used = db::project_invitation::seats_taken(&mut *tx, project.id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    tx.commit()
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;

    let free = (PROJECT_SEATS - used).max(0);
    let on_team = matches!(plan_code(settings.get_ref(), &user).await, Ok(Some(code)) if can_share_projects(&code));
    Ok(JsonResponse::build()
        .set_item(Seats {
            seats_total: PROJECT_SEATS,
            seats_used: used,
            seats_free: free,
            can_invite: on_team && free > 0,
        })
        .ok("OK"))
}

#[derive(Debug, Serialize)]
struct PublicInvitation {
    project_name: String,
    invited_by: String,
    email: String,
    role: String,
    status: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// What the invitation page shows before the invitee signs in. Looked up by
/// token only, so knowing an invitation id is not enough.
#[tracing::instrument(name = "Show invitation", skip_all)]
#[get("/invitations/{token}")]
pub async fn show(
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let invitation =
        db::project_invitation::fetch_by_token_hash(pg_pool.get_ref(), &agent_token::hash(&path.0))
            .await
            .map_err(JsonResponse::internal_server_error)?
            .ok_or_else(|| JsonResponse::not_found("not found"))?;
    let project = db::project::fetch(pg_pool.get_ref(), invitation.project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("not found"))?;

    Ok(JsonResponse::build()
        .set_item(PublicInvitation {
            project_name: project.name,
            invited_by: invitation.invited_by_email.clone(),
            email: mask_email(&invitation.email),
            role: invitation.role.clone(),
            status: invitation.effective_status().to_string(),
            expires_at: invitation.expires_at,
        })
        .ok("OK"))
}

#[tracing::instrument(name = "My pending invitations", skip_all)]
#[get("/invitations/pending")]
pub async fn pending(
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let email = user.email.trim().to_lowercase();
    let invitations = db::project_invitation::pending_for_email(pg_pool.get_ref(), &email)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    Ok(JsonResponse::build().set_list(invitations).ok("OK"))
}

#[derive(Debug, Serialize)]
struct Accepted {
    project_id: i32,
    role: String,
}

/// Accept an invitation by its token (from the email link) or by its id (from
/// the pending list). Either way the signed-in account's confirmed email must
/// be the invited one: that, not knowing the token, is what protects it.
#[tracing::instrument(name = "Accept invitation", skip_all)]
#[post("/invitations/{key}/accept")]
pub async fn accept(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let key = path.into_inner().0;
    let id = match Uuid::parse_str(&key) {
        Ok(id) => id,
        Err(_) => {
            db::project_invitation::fetch_by_token_hash(pg_pool.get_ref(), &agent_token::hash(&key))
                .await
                .map_err(JsonResponse::internal_server_error)?
                .ok_or_else(|| JsonResponse::not_found("not found"))?
                .id
        }
    };

    let mut tx = pg_pool
        .begin()
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;
    let invitation = db::project_invitation::lock_by_id(&mut *tx, id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("not found"))?;

    if invitation.email != user.email.trim().to_lowercase() {
        return Err(JsonResponse::forbidden("email_mismatch"));
    }
    if !user.email_confirmed {
        return Err(JsonResponse::forbidden("email_not_confirmed"));
    }
    if invitation.effective_status() != "pending" {
        return Err(actix_web::error::ErrorGone(
            JsonResponse::<()>::build()
                .set_msg(format!("invitation_{}", invitation.effective_status()))
                .to_string(),
        ));
    }

    let project = db::project::fetch(pg_pool.get_ref(), invitation.project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("not found"))?;
    let added = project.user_id != user.id
        && db::project_invitation::insert_member(
            &mut *tx,
            invitation.project_id,
            &user.id,
            &invitation.role,
            &invitation.invited_by,
        )
        .await
        .map_err(JsonResponse::internal_server_error)?;
    if !added {
        // Already the owner or a member. The invitation is pointless now;
        // release its seat.
        db::project_invitation::set_status(&mut *tx, invitation.id, "revoked", None)
            .await
            .map_err(JsonResponse::internal_server_error)?;
        tx.commit()
            .await
            .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;
        return Err(JsonResponse::<()>::build().conflict("already_member"));
    }
    db::project_invitation::set_status(&mut *tx, invitation.id, "accepted", Some(&user.id))
        .await
        .map_err(JsonResponse::internal_server_error)?;
    tx.commit()
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;

    Ok(JsonResponse::build()
        .set_item(Accepted {
            project_id: invitation.project_id,
            role: invitation.role,
        })
        .ok("OK"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_team_plan_codes_can_share() {
        assert!(can_share_projects("plan-individual-monthly"));
        assert!(can_share_projects("plan-individual-yearly"));
        for code in [
            "plan-plus-monthly",
            "plan-basic-monthly",
            "plan-free-periodically",
            "",
            "individual",
        ] {
            assert!(!can_share_projects(code), "{code}");
        }
    }

    #[test]
    fn emails_are_normalised_and_junk_refused() {
        assert_eq!(
            normalize_email("  Colleague@Try.Direct ").as_deref(),
            Some("colleague@try.direct")
        );
        for junk in [
            "",
            "not-an-email",
            "@try.direct",
            "a@b",
            "a@.b",
            "a b@try.direct",
        ] {
            assert_eq!(normalize_email(junk), None, "{junk}");
        }
    }

    #[test]
    fn masked_email_keeps_the_domain_and_hides_the_name() {
        assert_eq!(mask_email("colleague@try.direct"), "c********@try.direct");
        assert_eq!(mask_email("ab@x.io"), "a***@x.io");
    }

    #[test]
    fn invitation_email_carries_the_link_and_escapes_html() {
        let email = invitation_email(
            "colleague@try.direct",
            "<acme>",
            "team@try.direct",
            "https://try.direct/invitations/abc",
        );
        assert_eq!(email.to, "colleague@try.direct");
        assert!(email.message.contains("https://try.direct/invitations/abc"));
        let html = email.html.unwrap();
        assert!(html.contains("&lt;acme&gt;") && !html.contains("<acme>"));
        assert!(html.contains("https://try.direct/invitations/abc"));
    }

    #[test]
    fn invitation_email_goes_to_the_notify_email_channel() {
        // notify consumes `notify.start.email.*.*`.
        let parts: Vec<&str> = EMAIL_ROUTING_KEY.split('.').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(&parts[..3], ["notify", "start", "email"]);
    }
}
