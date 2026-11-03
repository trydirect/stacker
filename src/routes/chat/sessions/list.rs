use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct Query {
    pub project_id: Option<i32>,
    /// `?archived=true` returns the archived (soft-closed) sessions; omitted or
    /// `false` returns active sessions (the default nav view).
    pub archived: Option<bool>,
}

/// GET /chat/sessions[?project_id={id}][&archived=true]
/// Lists the logged-in user's chat sessions, newest-first. Returns metadata
/// only (id, title, archived_at, timestamps) — never message content,
/// encrypted or not. Active sessions by default; archived ones with
/// `?archived=true`.
#[tracing::instrument(name = "List chat sessions.", skip_all)]
#[get("/sessions")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    query: web::Query<Query>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let filter = db::chat_session::ArchivedFilter::from_query(query.archived);
    db::chat_session::list(pg_pool.get_ref(), &user.id, query.project_id, filter)
        .await
        .map(|list| JsonResponse::build().set_list(list).ok("OK"))
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))
}
