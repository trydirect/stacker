use std::sync::Arc;
use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use crate::forms::CloudForm;
use tracing::Instrument;

#[tracing::instrument(name = "Get cloud credentials.")]
#[get("/{id}")]
pub async fn item(
    path: web::Path<(i32,)>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    db::cloud::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|_err| JsonResponse::<models::Cloud>::build()
            .internal_server_error(""))
        .and_then(|cloud| {
            match cloud {
                Some(cloud) if cloud.user_id != user.id => {
                    Err(JsonResponse::not_found("record not found"))
                },
                Some(cloud) => {
                    let cloud = CloudForm::decode_model(cloud, false);
                    Ok(JsonResponse::build().set_item(Some(cloud)).ok("OK"))
                },
                None => Err(JsonResponse::not_found("record not found")),
            }
        })

}

#[tracing::instrument(name = "Get all clouds.")]
#[get("")]
pub async fn list(
    path: web::Path<()>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::cloud::fetch_by_user(pg_pool.get_ref(), user.id.as_ref())
        .await
        .map(|clouds| {

            let clouds = clouds
                .into_iter()
                .map(|cloud| CloudForm::decode_model(cloud, false) )
                // .map_err(|e| tracing::error!("Failed to decode cloud, {:?}", e))
                .collect();

            JsonResponse::build().set_list(clouds).ok("OK")

        })
        .map_err(|_err| JsonResponse::<models::Cloud>::build().internal_server_error(""))
}
