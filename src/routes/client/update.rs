use crate::helpers::client;
use crate::models;
use crate::db;
use crate::{configuration::Settings, helpers::JsonResponse};
use actix_web::{put, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;
use futures::TryFutureExt;

#[tracing::instrument(name = "Update client.")]
#[put("/{id}")]
pub async fn update_handler(
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

    client.secret = client::generate_secret(pool.get_ref(), 255)
        .await
        .map(|s| Some(s))
        .map_err(|msg| JsonResponse::<models::Client>::build().bad_request(msg))?;

    db::client::update(pool.get_ref(), client)
        .await
        .map(|client| JsonResponse::<models::Client>::build().set_item(client).ok("success"))
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            JsonResponse::<models::Client>::build().internal_server_error("")
        })
}
