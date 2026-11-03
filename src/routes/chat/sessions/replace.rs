use crate::db;
use crate::helpers::{self, JsonResponse};
use crate::models;
use actix_web::{put, web, Responder, Result};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ReplaceMessagesRequest {
    /// The full message list to persist. Encrypted at rest before storage.
    pub messages: Value,
}

/// PUT /chat/sessions/{id}/messages
/// Replaces a session's entire message history (idempotent upsert — the client
/// holds the full conversation and re-saves it). Owner-scoped; 404 if the
/// session does not exist or belongs to another user. Returns the stored
/// message list.
#[tracing::instrument(name = "Replace chat session messages.", skip_all)]
#[put("/sessions/{id}/messages")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    web::Json(body): web::Json<ReplaceMessagesRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();

    let encrypted = helpers::chat::encrypt_messages(&body.messages).map_err(|err| {
        JsonResponse::<models::ChatSession>::build()
            .internal_server_error(format!("Failed to encrypt messages: {err}"))
    })?;

    db::chat_session::update_messages(pg_pool.get_ref(), &id, &user.id, &encrypted)
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?
        .ok_or_else(|| {
            JsonResponse::<models::ChatSession>::build().not_found("Session not found")
        })?;

    Ok(JsonResponse::build().set_item(body.messages).ok("OK"))
}
