use crate::configuration::Settings;
use crate::db;
use crate::forms;
use crate::helpers::stack::builder::DcBuilder;
use crate::helpers::{JsonResponse, MqManager};
use crate::models;
use actix_web::{post, web, web::Data, Responder, Result};
use lapin::publisher_confirm::Confirmation;
use sqlx::PgPool;
use std::sync::Arc;
use crate::helpers::compressor::compress;


#[tracing::instrument(name = "Deploy for every user. Admin endpoint")]
#[post("/{id}/deploy")]
pub async fn add(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: Data<PgPool>,
    mq_manager: Data<MqManager>,
    sets: Data<Settings>,
) -> Result<impl Responder> {
    let id = path.0;
    let stack = db::stack::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(err))
        .and_then(|stack| match stack {
            Some(stack) if stack.user_id != user.id => Err(JsonResponse::<models::Client>::build().bad_request("client is not the owner")),
            Some(stack) => Ok(stack),
            None => Err(JsonResponse::<models::Stack>::build().not_found("not found")),
        })?;

    let id = stack.id.clone();
    let dc = DcBuilder::new(stack);
    let fc = dc.build().map_err(|err| {
        JsonResponse::<models::Stack>::build().internal_server_error(err)
    })?;

    let mut stack_data = forms::stack::Payload::try_from(&dc.stack)
        .map_err(|err| JsonResponse::<models::Stack>::build().bad_request(err))?;
    stack_data.user_token = Some(user.id.clone());
    stack_data.user_email = Some(user.email.clone());
    // let compressed = fc.unwrap_or("".to_string());
    stack_data.docker_compose = Some(compress(fc.as_str()));

    mq_manager
        .publish_and_confirm(
            "install".to_string(),
            "install.start.tfa.all.all".to_string(),
            &stack_data,
        )
        .await
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(err))
        .map(|_| {
            JsonResponse::<models::Stack>::build()
                .set_id(id)
                .ok("Success")
        })
}
