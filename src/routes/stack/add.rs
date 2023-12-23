use crate::db;
use crate::forms::stack::StackForm;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{
    post, web,
    web::{Bytes, Data},
    Responder, Result,
};
use chrono::Utc;
use serde_json::Value;
use serde_valid::Validate;
use sqlx::PgPool;
use std::str;
use std::sync::Arc;
use tracing::Instrument;
use uuid::Uuid;

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
    let form = serde_json::from_str::<StackForm>(body_str).map_err(|err| {
        let msg = format!("Invalid data. {:?}", err);
        JsonResponse::<StackForm>::build().bad_request(msg)
    })?;

    let stack_name = form.custom.custom_stack_code.clone();
    {
        let stack = db::stack::fetch_one_by_name(pool.get_ref(), &stack_name)
            .await
            .map_err(|err| {
                JsonResponse::<models::Stack>::build()
                    .internal_server_error("Internal Server Error")
            })?;
        if stack.is_some() {
            return Err(JsonResponse::<models::Stack>::build()
                .conflict("Stack with that name already exists"));
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
        return Err(JsonResponse::<models::Stack>::build().bad_request(errors.to_string()));
        // tmp solution
    }

    let body: Value = serde_json::to_value::<StackForm>(form)
        .or(serde_json::to_value::<StackForm>(StackForm::default()))
        .unwrap();

    let stack = models::Stack::new(user_id, stack_name, body);
    db::stack::insert(pool.get_ref(), stack)
        .await
        .map(|stack| JsonResponse::build().set_item(stack).ok("Ok"))
        .map_err(|_| {
            JsonResponse::<models::Stack>::build().internal_server_error("Internal Server Error")
        })
}
