use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{put, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[tracing::instrument(name = "User edit rating.")]
#[put("")]
pub async fn user_edit_handler(
    path: web::Path<(i32,)>,
    user: web::ReqData<Arc<models::User>>,
    form: web::Json<forms::rating::UserEdit>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let rate_id = path.0;
    let rating = db::rating::fetch(pg_pool.get_ref(), rate_id)
        .await
        .map_err(|_err| JsonResponse::<models::Rating>::build().internal_server_error(""))
        .and_then(|rating| {
            match rating {
                Some(rating) => { Ok(rating) },
                None => Err(JsonResponse::<models::Rating>::build().not_found("not found"))
            }
        })?;

    //todo check if form is valid
    //todo add update_model function to form
    //todo add the db saving of the model

    Ok(JsonResponse::build().set_item(rating).ok("OK"))
}
