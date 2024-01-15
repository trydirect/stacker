use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::convert::From;
use std::sync::Arc;
use tracing::Instrument;

#[tracing::instrument(name = "Get logged user stack.")]
#[get("/{id}")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// Get stack apps of logged user only
    let (id,) = path.into_inner();

    let stack = db::stack::fetch(pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(err))
        .and_then(|stack| match stack {
            Some(stack) if stack.user_id != user.id => {
                Err(JsonResponse::<models::Stack>::build().not_found("not found"))
            }
            Some(stack) => Ok(stack),
            None => Err(JsonResponse::<models::Stack>::build().not_found("not found")),
        })?;

    Ok(JsonResponse::build().set_item(Some(stack)).ok("OK"))
}

#[tracing::instrument(name = "Get user's stack list.")]
#[get("/user/{id}")]
pub async fn list(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// This is admin endpoint, used by a m2m app, client app is confidential
    /// it should return stacks by user id
    /// in order to pass validation at external deployment service
    let user_id = path.into_inner().0;

    db::stack::fetch_by_user(pool.get_ref(), &user_id)
        .await
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(err))
        .map(|stacks| JsonResponse::build().set_list(stacks).ok("OK"))
}
