use actix_web::{
    web,
    web::{Bytes, Data, Json},
    Responder, Result,
};

use crate::forms::stack::StackForm;
use crate::helpers::JsonResponse;
use crate::models::user::User;
use crate::models::Stack;
use actix_web::post;
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;
use std::str;
use tracing::Instrument;
use uuid::Uuid;
use crate::helpers::stack::builder::DcBuilder;

#[tracing::instrument(name = "Add stack.")]
#[post("")]
pub async fn add(
    body: Bytes,
    user: web::ReqData<User>,
    pool: Data<PgPool>,
) -> Result<impl Responder> {

    let body_bytes = actix_web::body::to_bytes(body).await.unwrap();
    let body_str = str::from_utf8(&body_bytes).unwrap();
    // method 1 let app_state: AppState = serde_json::from_str(body_str).unwrap();
    // method 2 let app_state = serde_json::from_str::<AppState>(body_str).unwrap();
    let form = match serde_json::from_str::<StackForm>(body_str) {
        Ok(f) => {
            f
        }
        Err(err) => {
            return Ok(Json(JsonResponse::<Stack>::not_valid("")));
        }
    };

    let user_id = user.id.clone();
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Validating a new stack", %request_id,
        commonDomain=?&form.common_domain,
        region=?&form.region,
        domainList=?&form.domain_list
    );
    // using `enter` is an async function
    let _request_span_guard = request_span.enter(); // ->exit

    tracing::info!(
        "request_id {} Adding '{}' '{}' as a new stack",
        request_id,
        form.common_domain,
        form.region
    );

    let query_span = tracing::info_span!("Saving new stack details into the database");

    let stack_name = form.custom.custom_stack_code.clone();
    let body: Value = match serde_json::to_value::<StackForm>(form) {
        Ok(body) => body,
        Err(err) => {
            tracing::error!("request_id {} unwrap body {:?}", request_id, err);
            serde_json::to_value::<StackForm>(StackForm::default()).unwrap()
        }
    };

    let stack = Stack {
        id: 0_i32,                // internal stack id
        stack_id: Uuid::new_v4(), // public uuid of the stack
        user_id: user_id,         //
        name: stack_name,
        body: body,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    tracing::debug!("stack object {:?}", stack);
    return match sqlx::query!(
        r#"
        INSERT INTO user_stack (id, stack_id, user_id, name, body, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id;
        "#,
        0_i32,
        stack.stack_id,
        stack.user_id,
        stack.name,
        stack.body,
        stack.created_at,
        stack.updated_at
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(record) => {
            tracing::info!(
                "req_id: {} New stack details have been saved to database",
                request_id
            );
            Ok(Json(JsonResponse::<Stack>::ok(record.id, "Object saved")))
        }
        Err(err) => {
            tracing::error!("req_id: {} Failed to execute query: {:?}", request_id, err);
            Ok(Json(JsonResponse::<Stack>::not_valid("Failed to insert")))
        }
    };
}

#[tracing::instrument(name = "Generate docker-compose.")]
#[post("/{id}")]
pub async fn gen(
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
            let mut dc = DcBuilder::new();
            let fc = dc.build(stack);
            tracing::debug!("Docker compose file content {:?}", fc.unwrap());
        }
        None => {

        }
    }

    Ok(Json(JsonResponse::<Stack>::ok(1, "OK")))
}
