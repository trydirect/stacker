use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{delete, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// DELETE /chat/sessions/{id}
/// Deletes a session owned by the logged-in user. Deleting a session that does
/// not exist — or belongs to another user — returns 404.
#[tracing::instrument(name = "Delete chat session.", skip_all)]
#[delete("/sessions/{id}")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();

    let affected = db::chat_session::delete(pg_pool.get_ref(), &id, &user.id)
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;

    if affected == 0 {
        return Err(JsonResponse::<models::ChatSession>::build().not_found("Session not found"));
    }

    Ok(JsonResponse::<models::ChatSessionSummary>::build().ok("OK"))
}
