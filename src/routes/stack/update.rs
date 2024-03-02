use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{web, web::Data, Responder, Result, post};
use serde_json::Value;
use serde_valid::Validate;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;
use uuid::Uuid;

#[tracing::instrument(name = "Update stack.")]
#[post("/{id}")]
pub async fn update(
    path: web::Path<(i32,)>,
    form: web::Json<forms::stack::Stack>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    let mut stack = db::stack::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(err))
        .and_then(|stack| match stack {
            Some(stack) => Ok(stack),
            None => Err(JsonResponse::<models::Stack>::build().not_found("Object not found")),
        })?;

    if stack.user_id != user.id {
        return Err(JsonResponse::<models::Client>::build().bad_request("client is not the owner"));
    }

    let stack_name = form.custom.custom_stack_code.clone();
    tracing::debug!("form data: {:?}", form);
    let user_id = user.id.clone();

    if let Err(errors) = form.validate() { 
        return Err(JsonResponse::<models::Stack>::build().form_error(errors.to_string()));
    }

    let form_inner = form.into_inner();

    if !form_inner.is_readable_docker_image().await.is_ok() {
        return Err(JsonResponse::<models::Stack>::build().bad_request("Can not access docker image"));
    }

    let body: Value = serde_json::to_value::<forms::stack::Stack>(form_inner)
        .map_err(|err| 
            JsonResponse::<models::Stack>::build().bad_request(format!("{err}"))
        )?; 

    stack.stack_id = Uuid::new_v4();
    stack.user_id = user_id;
    stack.name = stack_name;
    stack.body = body;

    db::stack::update(pg_pool.get_ref(), stack)
        .await
        .map(|stack| {
            JsonResponse::<models::Stack>::build()
                .set_item(stack)
                .ok("success")
        })
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            JsonResponse::<models::Stack>::build().internal_server_error("")
        })
}
