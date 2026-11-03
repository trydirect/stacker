use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{post, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// POST /chat/sessions/{id}/archive
/// Archives (soft-closes) a session owned by the logged-in user. The thread is
/// kept and its messages preserved; it simply leaves the default active list
/// and moves into the archived view (`GET /chat/sessions?archived=true`). This
/// is the non-destructive alternative the "+ new chat" action should use
/// instead of DELETE. Returns the updated session summary. 404 if the session
/// does not exist or belongs to another user.
#[tracing::instrument(name = "Archive chat session.", skip_all)]
#[post("/sessions/{id}/archive")]
pub async fn archive(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    set_archived(&pg_pool, &user.id, path.into_inner(), true).await
}

/// POST /chat/sessions/{id}/unarchive
/// Restores an archived session back to the active list.
#[tracing::instrument(name = "Unarchive chat session.", skip_all)]
#[post("/sessions/{id}/unarchive")]
pub async fn unarchive(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    set_archived(&pg_pool, &user.id, path.into_inner(), false).await
}

async fn set_archived(
    pg_pool: &PgPool,
    user_id: &str,
    id: Uuid,
    archived: bool,
) -> Result<impl Responder> {
    db::chat_session::set_archived(pg_pool, &id, user_id, archived)
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?
        .map(|session| {
            JsonResponse::build()
                .set_item(models::ChatSessionSummary::from(session))
                .ok("OK")
        })
        .ok_or_else(|| {
            JsonResponse::<models::ChatSessionSummary>::build().not_found("Session not found")
        })
}
