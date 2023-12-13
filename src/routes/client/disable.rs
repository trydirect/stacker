use crate::configuration::Settings;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{put, web, Responder, Result};
use sqlx::PgPool;
use tracing::Instrument;
use std::sync::Arc;

#[tracing::instrument(name = "Disable client.")]
#[put("/{id}/disable")]
pub async fn disable_handler(
    user: web::ReqData<Arc<models::User>>,
    settings: web::Data<Settings>,
    pool: web::Data<PgPool>,
    path: web::Path<(i32,)>,
) -> Result<impl Responder> {
        let client_id = path.0;
        let mut client = db::client::fetch(pool.get_ref(), client_id)
            .await
            .map_err(|msg| JsonResponse::<models::Client>::build().not_found(msg))?; 
        if client.secret.is_none() {
            return Err(JsonResponse::<models::Client>::build().bad_request("client is not active"));
        }

        client.secret = None;
        db::client::update(pool.get_ref(), client)
            .await
            .map(|client| JsonResponse::build().set_item(client).ok("success"))
            .map_err(|msg| JsonResponse::<models::Client>::build().bad_request(msg))
}
