use crate::db;
use crate::helpers::{self, JsonResponse};
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// GET /chat/sessions/{id}/messages
/// Returns the decrypted message history of a session owned by the logged-in
/// user. A session that does not exist — or belongs to another user — returns
/// 404 (no cross-user existence leak).
#[tracing::instrument(name = "Get chat session messages.", skip_all)]
#[get("/sessions/{id}/messages")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();

    let session = db::chat_session::fetch(pg_pool.get_ref(), &id, &user.id)
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?
        .ok_or_else(|| {
            JsonResponse::<models::ChatSession>::build().not_found("Session not found")
        })?;

    let messages = helpers::chat::decrypt_messages(&session.messages_encrypted).map_err(|err| {
        JsonResponse::<models::ChatSession>::build()
            .internal_server_error(format!("Failed to decrypt messages: {err}"))
    })?;

    Ok(JsonResponse::build().set_item(messages).ok("OK"))
}
