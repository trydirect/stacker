use crate::helpers::{JsonResponse, JsonResponseBuilder};
use crate::models;
use crate::models::user::User;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::convert::From;
use tracing::Instrument;

#[tracing::instrument(name = "Get logged user stack.")]
#[get("/{id}")]
pub async fn item(
    user: web::ReqData<User>,
    path: web::Path<(i32,)>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// Get stack apps of logged user only
    let (id,) = path.into_inner();

    tracing::info!("User {:?} gets stack by id {:?}", user.id, id);
    match sqlx::query_as!(
        models::Stack,
        r#"
        SELECT * FROM user_stack WHERE id=$1 AND user_id=$2 LIMIT 1
        "#,
        id,
        user.id
    )
    .fetch_one(pool.get_ref())
    .await
    {
        Ok(stack) => {
            tracing::info!("Stack found: {:?}", stack.id,);
            return JsonResponse::build().set_item(Some(stack)).ok("OK");
        }
        Err(sqlx::Error::RowNotFound) => JsonResponse::build().not_found("Record not found"),
        Err(e) => {
            tracing::error!("Failed to fetch stack, error: {:?}", e);
            return JsonResponse::build().internal_server_error("Could not fetch data");
        }
    }
}

#[tracing::instrument(name = "Get user's stack list.")]
#[get("/user/{id}")]
pub async fn list(
    user: web::ReqData<User>,
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
            return JsonResponse::build().set_list(list).ok("OK");
        }
        Err(sqlx::Error::RowNotFound) => {
            tracing::error!("No stacks found for user: {:?}", &user.id);
            return JsonResponse::build().not_found("No stacks found for user");
        }
        Err(e) => {
            tracing::error!("Failed to fetch stack, error: {:?}", e);
            return JsonResponse::build().internal_server_error("Could not fetch");
        }
    }
}
