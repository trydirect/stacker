use crate::db;
use crate::helpers::{self, JsonResponse};
use crate::models;
use actix_web::{post, web, Responder, Result};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub project_id: Option<i32>,
    pub title: Option<String>,
    /// Optional initial messages. Encrypted at rest before storage.
    pub messages: Option<Value>,
}

/// POST /chat/sessions
/// Creates a new chat session (dialog) for the logged-in user. Any initial
/// messages are encrypted (AES-256-GCM) before being written to the database.
#[tracing::instrument(name = "Create chat session.", skip_all)]
#[post("/sessions")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    web::Json(body): web::Json<CreateSessionRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let messages = body.messages.unwrap_or_else(|| Value::Array(vec![]));

    let encrypted = helpers::chat::encrypt_messages(&messages).map_err(|err| {
        JsonResponse::<models::ChatSessionSummary>::build()
            .internal_server_error(format!("Failed to encrypt messages: {err}"))
    })?;

    db::chat_session::create(
        pg_pool.get_ref(),
        &user.id,
        body.project_id,
        body.title.as_deref(),
        &encrypted,
    )
    .await
    .map(|session| {
        JsonResponse::build()
            .set_item(models::ChatSessionSummary::from(session))
            .created("Created")
    })
    .map_err(|err| JsonResponse::internal_server_error(err.to_string()))
}
