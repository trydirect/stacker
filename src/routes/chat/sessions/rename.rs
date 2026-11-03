use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{patch, web, Responder, Result};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct RenameSessionRequest {
    /// New title. `null` clears the title.
    pub title: Option<String>,
}

/// PATCH /chat/sessions/{id}
/// Updates the title of a session owned by the logged-in user. Returns the
/// updated session summary (no message content). 404 if the session does not
/// exist or belongs to another user.
#[tracing::instrument(name = "Rename chat session.", skip_all)]
#[patch("/sessions/{id}")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    web::Json(body): web::Json<RenameSessionRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();

    db::chat_session::update_title(pg_pool.get_ref(), &id, &user.id, body.title.as_deref())
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
