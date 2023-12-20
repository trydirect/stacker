use crate::forms::stack::StackForm;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{
    post,
    web,
    web::{Bytes, Data},
    Responder, Result,
};
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;
use std::str;
use serde_valid::Validate;
use tracing::Instrument;
use uuid::Uuid;
use std::sync::Arc;

#[tracing::instrument(name = "Add stack.")]
#[post("")]
pub async fn add(
    body: Bytes,
    user: web::ReqData<Arc<models::User>>,
    pool: Data<PgPool>,
) -> Result<impl Responder> {

    // @todo ACL
    let body_bytes = actix_web::body::to_bytes(body).await.unwrap();
    let body_str = str::from_utf8(&body_bytes).unwrap();
    let form = serde_json::from_str::<StackForm>(body_str)
        .map_err(|err| {
            let msg = format!("Invalid data. {:?}", err);
            JsonResponse::<StackForm>::build().bad_request(msg)
        })?;

    let stack_name = form.custom.custom_stack_code.clone();
    {
        let stack = db::stack::fetch_one_by_name(pool.get_ref(), &stack_name)
            .await
            .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error("Internal Server Error"))?;
        if stack.is_some() {
            return Err(JsonResponse::<models::Stack>::build().conflict("Stack with that name already exists"));
        }
    }

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

    if !form.validate().is_ok() {
        let errors = form.validate().unwrap_err();
        let err_msg = format!("Invalid data received {:?}", &errors.to_string());
        tracing::debug!(err_msg);
        return Err(JsonResponse::<models::Stack>::build().bad_request(errors.to_string()));// tmp solution
    }

    let body: Value = serde_json::to_value::<StackForm>(form)
        .or(serde_json::to_value::<StackForm>(StackForm::default()))
        .unwrap();

    match sqlx::query!(
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
            return Ok(JsonResponse::<models::Stack>::build().set_id(record.id).ok("OK"));
        }
        Err(e) => {
            tracing::error!("req_id: {} Failed to execute query: {:?}", request_id, e);
            return Err(JsonResponse::<models::Stack>::build().internal_server_error("Internal Server Error"));
        }
    };
}
