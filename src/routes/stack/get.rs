use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::convert::From;
use tracing::Instrument;
use std::sync::Arc;

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
        .map_err(|err| JsonResponse::<models::Stack>::build().not_found("Record not found"))?;

    if stack.user_id != user.id {
        return Err(JsonResponse::<models::Stack>::build().bad_request("Forbidden"));
    }

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
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(""))
        .map(|stacks| JsonResponse::build().set_list(stacks).ok("OK"))
}
