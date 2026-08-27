use crate::db;
use crate::helpers::{self, JsonResponse};
use crate::models;
use actix_web::{post, web, Responder, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct AppendMessageRequest {
    pub role: String,
    pub content: String,
    /// Any additional fields the client sends (e.g. `id`, `timestamp`, model,
    /// tokens, tool calls) are captured here and stored verbatim alongside
    /// `role`/`content`, so the message round-trips unchanged.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// POST /chat/sessions/{id}/messages
/// Appends a single message to a session owned by the logged-in user. The
/// existing blob is decrypted, the new message pushed, and the result
/// re-encrypted (read-modify-write — concurrent appends to the same session
/// last-writer-wins, acceptable for a single user's own dialog).
/// Returns the full, updated message list.
#[tracing::instrument(name = "Append chat session message.", skip_all)]
#[post("/sessions/{id}/messages")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    web::Json(body): web::Json<AppendMessageRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();

    // Load current (owner-scoped) — 404 if it isn't the caller's session.
    let session = db::chat_session::fetch(pg_pool.get_ref(), &id, &user.id)
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?
        .ok_or_else(|| {
            JsonResponse::<models::ChatSession>::build().not_found("Session not found")
        })?;

    let mut messages = helpers::chat::decrypt_messages(&session.messages_encrypted)
        .map_err(|err| {
            JsonResponse::<models::ChatSession>::build()
                .internal_server_error(format!("Failed to decrypt messages: {err}"))
        })?;

    let mut new_message = json!({ "role": body.role, "content": body.content });
    if let Value::Object(ref mut map) = new_message {
        // Preserve client-supplied fields (id, timestamp, …) verbatim.
        for (k, v) in body.extra {
            map.entry(k).or_insert(v);
        }
    }

    match messages {
        Value::Array(ref mut arr) => arr.push(new_message),
        _ => messages = Value::Array(vec![new_message]),
    }

    let encrypted = helpers::chat::encrypt_messages(&messages).map_err(|err| {
        JsonResponse::<models::ChatSession>::build()
            .internal_server_error(format!("Failed to encrypt messages: {err}"))
    })?;

    db::chat_session::update_messages(pg_pool.get_ref(), &id, &user.id, &encrypted)
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?
        .ok_or_else(|| {
            // Session disappeared between fetch and update.
            JsonResponse::<models::ChatSession>::build().not_found("Session not found")
        })?;

    Ok(JsonResponse::build().set_item(messages).ok("OK"))
}
