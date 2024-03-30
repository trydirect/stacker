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
use chrono::Utc;

#[tracing::instrument(name = "Update cloud.")]
#[put("/{id}")]
pub async fn item(
    path: web::Path<(i32,)>,
    form: web::Json<forms::cloud::CloudForm>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {

    let id = path.0;
    let cloud_row = db::cloud::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Cloud>::build().internal_server_error(err))
        .and_then(|cloud| match cloud {
            Some(cloud) if cloud.user_id != user.id => {
                Err(JsonResponse::<models::Project>::build().bad_request("Cloud not found"))
            }
            Some(cloud) => Ok(cloud),
            None => Err(JsonResponse::<models::Cloud>::build().not_found("Cloud not found")),
        })?;

    if let Err(errors) = form.validate() {
        return Err(JsonResponse::<models::Cloud>::build().form_error(errors.to_string()));
    }

    let mut cloud:models::Cloud = form.deref().into();
    cloud.id = cloud_row.id;
    cloud.user_id = user.id.clone();
    // cloud.updated_at = Utc::now();

    tracing::debug!("Updating cloud {:?}", cloud);

    db::cloud::update(pg_pool.get_ref(), cloud)
        .await
        .map(|cloud| {
            JsonResponse::<models::Cloud>::build()
                .set_item(cloud)
                .ok("success")
        })
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            JsonResponse::<models::Cloud>::build().internal_server_error("Could not update")
        })
}
