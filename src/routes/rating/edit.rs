use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{put, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use serde_valid::Validate;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[tracing::instrument(name = "User edit rating.")]
#[put("/{id}")]
pub async fn user_edit_handler(
    path: web::Path<(i32,)>,
    user: web::ReqData<Arc<models::User>>,
    form: web::Json<forms::rating::UserEdit>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    if let Err(errors) = form.validate() {
        return Err(JsonResponse::<models::Rating>::build().form_error(errors.to_string()));
    }

    let rate_id = path.0;
    let mut rating = db::rating::fetch(pg_pool.get_ref(), rate_id)
        .await
        .map_err(|_err| JsonResponse::<models::Rating>::build().internal_server_error(""))
        .and_then(|rating| {
            match rating {
                Some(rating) if rating.user_id == user.id=>  Ok(rating),
                _ => Err(JsonResponse::<models::Rating>::build().not_found("not found"))
            }
        })?;

    form.into_inner().update(&mut rating);

    db::rating::update(pg_pool.get_ref(), rating)
        .await
        .map(|rating| {
            JsonResponse::<models::Rating>::build()
                .set_item(rating)
                .ok("success")
        })
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            JsonResponse::<models::Rating>::build().internal_server_error("Rating not update")
        })
}
