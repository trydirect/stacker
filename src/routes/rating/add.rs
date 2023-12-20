use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{post, web, Responder, Result};
use sqlx::PgPool;
use tracing::Instrument;
use std::sync::Arc;
use futures::TryFutureExt;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[tracing::instrument(name = "Add rating.")]
#[post("")]
pub async fn add_handler(
    user: web::ReqData<Arc<models::User>>,
    form: web::Json<forms::Rating>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let _product = db::product::fetch_by_obj(pg_pool.get_ref(), form.obj_id)
        .await
        .map_err(|msg| JsonResponse::<models::Rating>::build().internal_server_error(msg))?
        .ok_or_else(|| JsonResponse::<models::Rating>::build().not_found("not found"))?
        ; 

    match db::rating::fetch_by_obj_and_user_and_category(pg_pool.get_ref(), form.obj_id, user.id.clone(), form.category).await {
        Ok(record) => {
            return Err(JsonResponse::<models::Rating>::build().conflict("Already rated"));
        }
        Err(_e) => {}
    }

    let mut rating: models::Rating = form.into_inner().into();
    rating.user_id = user.id.clone();

    db::rating::insert(pg_pool.get_ref(), rating)
        .await
        .map(|rating| JsonResponse::build().set_item(rating).ok("success"))
        .map_err(|err| JsonResponse::<models::Rating>::build().internal_server_error("Failed to insert"))
}
