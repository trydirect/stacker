use actix_web::{
    web,
    web::{Data, Json},
    Responder, Result,
};

use crate::helpers::JsonResponse;
use crate::models::user::User;
use crate::models::Stack;
use actix_web::{get, post};
use sqlx::PgPool;
use std::str;
use tracing::Instrument;
use uuid::Uuid;
use crate::helpers::stack::builder::DcBuilder;

#[tracing::instrument(name = "User's generate docker-compose.")]
#[post("/{id}")]
pub async fn add(
    user: web::ReqData<User>,
    path: web::Path<(i32,)>,
    pool: Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    tracing::debug!("Received id: {}", id);

    let stack = match sqlx::query_as!(
        Stack,
        r#"
        SELECT * FROM user_stack WHERE id=$1 AND user_id=$2 LIMIT 1
        "#,
        id, user.id
    )
        .fetch_one(pool.get_ref())
        .await
    {
        Ok(stack) => {
            tracing::info!("stack found: {:?}", stack.id,);
            Some(stack)
        }
        Err(sqlx::Error::RowNotFound) => {
            tracing::error!("Row not found 404");
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
            let mut dc = DcBuilder::new(stack);
            let fc = dc.build();
            // tracing::debug!("Docker compose file content {:?}", fc.unwrap());
            return Ok(Json(JsonResponse::new(
                "OK".to_owned(),
                "Success".to_owned(),
                200,
                Some(id),
                Some(fc.unwrap()),
                None
            )));

        }
        None => {
            return Ok(Json(JsonResponse::internal_error("Could not generate compose file")));
        }
    }
}

#[tracing::instrument(name = "Generate docker-compose. Admin")]
#[get("/{id}/compose")]
pub async fn admin(
    user: web::ReqData<User>,
    path: web::Path<(i32,)>,
    pool: Data<PgPool>,
) -> Result<impl Responder> {
    ///  Admin function for generating compose file for specified user
    let id = path.0;
    tracing::debug!("Received id: {}", id);

    let stack = match sqlx::query_as!(
        Stack,
        r#"
        SELECT * FROM user_stack WHERE id=$1 LIMIT 1
        "#,
        id,
    )
        .fetch_one(pool.get_ref())
        .await
    {
        Ok(stack) => {
            tracing::info!("stack found: {:?}", stack.id,);
            Some(stack)
        }
        Err(sqlx::Error::RowNotFound) => {
            tracing::error!("Row not found 404");
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
            let mut dc = DcBuilder::new(stack);
            let fc = dc.build();
            // tracing::debug!("Docker compose file content {:?}", fc.unwrap());
            return Ok(Json(JsonResponse::new(
                "OK".to_owned(),
                "Success".to_owned(),
                200,
                Some(id),
                Some(fc.unwrap()),
                None
            )));

        }
        None => {
            return Ok(Json(JsonResponse::internal_error("Could not generate compose file")));
        }
    }
}
