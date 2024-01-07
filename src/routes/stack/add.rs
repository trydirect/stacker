use crate::db;
use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::Error;
use actix_web::{
    post, web,
    web::{Bytes, Data},
    Responder, Result,
};
use serde_json::Value;
use serde_valid::Validate;
use sqlx::PgPool;
use std::str;
use std::sync::Arc;

#[tracing::instrument(name = "Add stack.")]
#[post("")]
pub async fn add(
    body: Bytes,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    // @todo ACL
    let form = body_into_form(body).await?;
    let stack_name = form.custom.custom_stack_code.clone();

    check_if_stack_exists(pg_pool.get_ref(), &stack_name).await?;

    let body: Value = serde_json::to_value::<forms::StackForm>(form)
        .or(serde_json::to_value::<forms::StackForm>(forms::StackForm::default()))
        .unwrap();

    let stack = models::Stack::new(user.id.clone(), stack_name, body);
    db::stack::insert(pg_pool.get_ref(), stack)
        .await
        .map(|stack| JsonResponse::build().set_item(stack).ok("Ok"))
        .map_err(|_| {
            JsonResponse::<models::Stack>::build().internal_server_error("Internal Server Error")
        })
}

async fn check_if_stack_exists(pg_pool: &PgPool, stack_name: &String) -> Result<(), Error> {
    db::stack::fetch_one_by_name(pg_pool, stack_name)
        .await
        .map_err(|_| {
            JsonResponse::<models::Stack>::build().internal_server_error("Internal Server Error")
        })
        .and_then(|stack| match stack {
            Some(_) => Err(JsonResponse::<models::Stack>::build()
                .conflict("Stack with that name already exists")),
            None => Ok(()),
        })
}

async fn body_into_form(body: Bytes) -> Result<forms::StackForm, Error> {
    let body_bytes = actix_web::body::to_bytes(body).await.unwrap();
    let body_str = str::from_utf8(&body_bytes)
        .map_err(|err| JsonResponse::<forms::StackForm>::build().internal_server_error(err.to_string()))?;
    let deserializer = &mut serde_json::Deserializer::from_str(body_str);
    serde_path_to_error::deserialize(deserializer)
        .map_err(|err| {
            let msg = format!("{}:{:?}", err.path().to_string(), err);
            JsonResponse::<forms::StackForm>::build().bad_request(msg)
        })
        .and_then(|form: forms::StackForm| {
            if !form.validate().is_ok() {
                let errors = form.validate().unwrap_err();
                let err_msg = format!("Invalid data received {:?}", &errors.to_string());
                tracing::debug!(err_msg);
                return Err(JsonResponse::<models::Stack>::build().bad_request(errors.to_string()));
            }

            Ok(form)
        })
}
