use crate::forms;
use crate::models;
use crate::models::user::User;
use crate::models::RateCategory;
use actix_web::get;
use actix_web::{web, Responder, Result};
use serde_derive::Serialize;
use sqlx::PgPool;
use tracing::Instrument;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[derive(Serialize)]
struct JsonResponse {
    status: String,
    message: String,
    code: u32,
    id: Option<i32>,
}

#[tracing::instrument(name = "Get rating.")]
#[get("/{id}")]
pub async fn get_handler(
    path: web::Path<(u32,)>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    Ok(web::Json(JsonResponse {
        status: "error".to_string(),
        code: 500,
        message: "Failed to insert".to_string(),
        id: None,
    }))
}
