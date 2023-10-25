use crate::models;
use crate::models::user::User;
use crate::models::RateCategory;
use crate::utils::json::JsonResponse;
use actix_web::post;
use actix_web::{web, Responder, Result};
use sqlx::PgPool;
use tracing::Instrument;

#[tracing::instrument(name = "Add client.")]
#[post("")]
pub async fn add_handler(
    user: web::ReqData<User>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    //todo generate client
    //save client in DB
    //return the client as a response
}
