use crate::forms::stack::StackForm;
use crate::helpers::JsonResponse;
use crate::models;
use crate::models::user::User;
use actix_web::post;
use actix_web::{
    web,
    web::{Bytes, Data},
    Responder, Result,
};
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;
use std::str;
use tracing::Instrument;
use uuid::Uuid;

#[tracing::instrument(name = "Add stack.")]
#[post("")]
pub async fn add(
    body: Bytes,
    user: web::ReqData<User>,
    pool: Data<PgPool>,
) -> Result<impl Responder> {
    let body_bytes = actix_web::body::to_bytes(body).await.unwrap();
    let body_str = str::from_utf8(&body_bytes).unwrap();
    let form = match serde_json::from_str::<StackForm>(body_str) {
        Ok(f) => f,
        Err(_err) => {
            let msg = format!("Invalid data. {:?}", _err);
            return JsonResponse::<StackForm>::build().bad_request("Invalid data");
        }
    };

    let user_id = user.id.clone();
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Validating a new stack", %request_id,
        commonDomain=?&form.custom.project_name,
        region=?&form.region,
        domainList=?&form.domain_list
    );
    // using `enter` is an async function
    let _request_span_guard = request_span.enter(); // ->exit

    tracing::info!(
        "request_id {} Adding '{}' '{}' as a new stack",
        request_id,
        form.custom.project_name,
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

    return match sqlx::query!(
        r#"
        INSERT INTO user_stack (stack_id, user_id, name, body, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id;
        "#,
        Uuid::new_v4(),
        user_id,
        stack_name,
        body,
        Utc::now(),
        Utc::now(),
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
            return JsonResponse::build().set_id(record.id).ok("OK");
        }
        Err(e) => {
            tracing::error!("req_id: {} Failed to execute query: {:?}", request_id, e);
            return JsonResponse::build().bad_request("Internal Server Error");
        }
    };
}
