use crate::configuration::Settings;
use crate::db;
use crate::helpers;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{put, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;

#[tracing::instrument(name = "Enable client.")]
#[put("/{id}/enable")]
pub async fn enable_handler(
    user: web::ReqData<Arc<models::User>>,
    settings: web::Data<Settings>,
    pg_pool: web::Data<PgPool>,
    path: web::Path<(i32,)>,
) -> Result<impl Responder> {
    println!("user.id {}", user.id);
    //todo the owner
    //todo add admin endpoint
    let client_id = path.0;
    let mut client = db::client::fetch(pg_pool.get_ref(), client_id)
        .await
        .map_err(|msg| JsonResponse::<models::Client>::build().internal_server_error(msg))?
        .ok_or_else(|| JsonResponse::<models::Client>::build().not_found("not found"))?;

    if client.secret.is_some() {
        return Err(JsonResponse::<models::Client>::build().bad_request("client is already active"));
    }

    client.secret = helpers::client::generate_secret(pg_pool.get_ref(), 255)
        .await
        .map(|secret| Some(secret))
        .map_err(|err| JsonResponse::<models::Client>::build().bad_request(err))?;

    db::client::update(pg_pool.get_ref(), client)
        .await
        .map(|client| JsonResponse::build().set_item(client).ok("success"))
        .map_err(|err| JsonResponse::<models::Client>::build().bad_request(err))
}
