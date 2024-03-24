use std::sync::Arc;
use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[tracing::instrument(name = "Get cloud.")]
#[get("/{id}")]
pub async fn item(
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    let cloud = db::cloud::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|_err| JsonResponse::<models::Cloud>::build()
            .internal_server_error(""))
        .and_then(|cloud| {
            match cloud {
                Some(cloud) => { Ok(cloud) },
                None => Err(JsonResponse::<models::Cloud>::build().not_found("object not found"))
            }
        })?;

    Ok(JsonResponse::build().set_item(cloud).ok("OK"))
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
        .map(|clouds| JsonResponse::build().set_list(clouds).ok("OK"))
        .map_err(|_err| JsonResponse::<models::Cloud>::build().internal_server_error(""))
}
