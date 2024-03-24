use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{web, web::Data, Responder, Result, put};
use serde_valid::Validate;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;
use std::ops::Deref;

#[tracing::instrument(name = "Update server.")]
#[put("/{id}")]
pub async fn item(
    path: web::Path<(i32,)>,
    form: web::Json<forms::server::Server>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {

    let id = path.0;
    let server_row = db::server::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Server>::build().internal_server_error(err))
        .and_then(|server| match server {
            Some(server) if server.user_id != user.id => {
                Err(JsonResponse::<models::Project>::build().bad_request("Server not found"))
            }
            Some(server) => Ok(server),
            None => Err(JsonResponse::<models::Server>::build().not_found("Server not found")),
        })?;

    if let Err(errors) = form.validate() {
        return Err(JsonResponse::<models::Server>::build().form_error(errors.to_string()));
    }

    let mut server:models::Server = form.deref().into();
    server.id = server_row.id;
    server.project_id = server_row.project_id;
    server.user_id = user.id.clone();
    // exclude
    // server.created_at

    tracing::debug!("Updating server {:?}", server);

    db::server::update(pg_pool.get_ref(), server)
        .await
        .map(|server| {
            JsonResponse::<models::Server>::build()
                .set_item(server)
                .ok("success")
        })
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            JsonResponse::<models::Server>::build().internal_server_error("Could not update server")
        })
}
