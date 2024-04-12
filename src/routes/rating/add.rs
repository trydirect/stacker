use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{post, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use serde_valid::Validate;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[tracing::instrument(name = "Add rating.")]
#[post("")]
pub async fn user_add_handler(
    user: web::ReqData<Arc<models::User>>,
    form: web::Json<forms::rating::Add>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    if let Err(errors) = form.validate() {
        return Err(JsonResponse::<models::Rating>::build().form_error(errors.to_string()));
    }

    let _product = db::product::fetch_by_obj(pg_pool.get_ref(), form.obj_id)
        .await
        .map_err(|_msg| JsonResponse::<models::Rating>::build().internal_server_error(_msg))?
        .ok_or_else(|| JsonResponse::<models::Rating>::build().not_found("not found"))?
        ; 

    let rating = db::rating::fetch_by_obj_and_user_and_category(
        pg_pool.get_ref(), form.obj_id, user.id.clone(), form.category)
        .await
        .map_err(|err| JsonResponse::<models::Rating>::build().internal_server_error(err))?;

    if rating.is_some() {
        return Err(JsonResponse::<models::Rating>::build().bad_request("already rated"));
    }

    let mut rating: models::Rating = form.into_inner().into();
    rating.user_id = user.id.clone();

    db::rating::insert(pg_pool.get_ref(), rating)
        .await
        .map(|rating| JsonResponse::build().set_item(rating).ok("success"))
        .map_err(|_err| JsonResponse::<models::Rating>::build()
            .internal_server_error("Failed to insert"))
}
