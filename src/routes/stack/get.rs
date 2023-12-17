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
    pool: web::Data<PgPool>, //&Web::Data<PgPool>
                             //get_ref
                             //*
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
    let (id,) = path.into_inner();
    tracing::info!("Logged user: {:?}", user.id);
    tracing::info!("Get stack list for user {:?}", id);

    let query_span = tracing::info_span!("Get stacks by user id.");

    match sqlx::query_as!(
        models::Stack,
        r#"
        SELECT * FROM user_stack WHERE user_id=$1
        "#,
        id
    )
    .fetch_all(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(list) => {
            return Ok(JsonResponse::build().set_list(list).ok("OK"));
        }
        Err(sqlx::Error::RowNotFound) => {
            tracing::error!("No stacks found for user: {:?}", &user.id);
            return Err(JsonResponse::<models::Stack>::build().not_found("No stacks found for user"));
        }
        Err(e) => {
            tracing::error!("Failed to fetch stack, error: {:?}", e);
            return Err(JsonResponse::<models::Stack>::build().internal_server_error("Could not fetch"));
        }
    }
}
