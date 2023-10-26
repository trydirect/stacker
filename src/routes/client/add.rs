use crate::models;
use crate::models::user::User;
use crate::models::RateCategory;
use crate::utils::json::JsonResponse;
use actix_web::{post, web, Responder, Result};
use serde_derive::Serialize;
use sqlx::PgPool;
use tracing::Instrument;

#[derive(Serialize)]
struct ClientAddResponse {
    status: String,
    code: u32,
}

#[tracing::instrument(name = "Add client.")]
#[post("")]
pub async fn add_handler(
    user: web::ReqData<User>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    //todo how many clients can an user have?
    //todo generate client
    //save client in DB
    //return the client as a response
    return Ok(web::Json(ClientAddResponse {
        status: "success".to_string(),
        code: 200,
    }));
}
