use actix_web::{get, web, web::Data, Responder, Result};

use crate::db;
use crate::helpers::stack::builder::DcBuilder;
use crate::helpers::JsonResponse;
use crate::models;
use sqlx::PgPool;
use std::sync::Arc;

#[tracing::instrument(name = "User's generate docker-compose.")]
#[get("/{id}")]
pub async fn add(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pool: Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
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

    let id = stack.id.clone();
    let dc = DcBuilder::new(stack);
    let fc = dc.build();
    tracing::debug!("Docker compose file content {:?}", fc);

    Ok(JsonResponse::build()
        .set_id(id)
        .set_item(fc.unwrap())
        .ok("Success"))
}

#[tracing::instrument(name = "Generate docker-compose. Admin")]
#[get("/{id}/compose")]
pub async fn admin(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pool: Data<PgPool>,
) -> Result<impl Responder> {
    ///  Admin function for generating compose file for specified user
    let id = path.0;
    tracing::debug!("Received id: {}", id);

    let stack = match sqlx::query_as!(
        models::Stack,
        r#"
        SELECT * FROM user_stack WHERE id=$1 LIMIT 1
        "#,
        id,
    )
    .fetch_one(pool.get_ref())
    .await
    {
        Ok(stack) => {
            tracing::info!("Record found: {:?}", stack.id);
            Some(stack)
        }
        Err(sqlx::Error::RowNotFound) => {
            tracing::error!("Record not found");
            None
        }
        Err(e) => {
            tracing::error!("Failed to fetch stack, error: {:?}", e);
            None
        }
    };

    match stack {
        Some(stack) => {
            let id = stack.id.clone();
            let dc = DcBuilder::new(stack);
            let fc = match dc.build() {
                Some(fc) => fc,
                None => {
                    tracing::error!("Error. Compose builder returned an empty string");
                    "".to_string()
                }
            };
            // tracing::debug!("Docker compose file content {:?}", fc);
            return Ok(JsonResponse::build().set_id(id).set_item(fc).ok("Success"));
        }
        None => {
            return Err(JsonResponse::<models::Stack>::build()
                .bad_request("Could not generate compose file"));
        }
    }
}
