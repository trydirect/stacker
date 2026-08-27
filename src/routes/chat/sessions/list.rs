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
}

/// GET /chat/sessions[?project_id={id}]
/// Lists the logged-in user's chat sessions, newest-first. Returns metadata
/// only (id, title, timestamps) — never message content, encrypted or not.
#[tracing::instrument(name = "List chat sessions.", skip_all)]
#[get("/sessions")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    query: web::Query<Query>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::chat_session::list(pg_pool.get_ref(), &user.id, query.project_id)
        .await
        .map(|list| JsonResponse::build().set_list(list).ok("OK"))
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))
}
