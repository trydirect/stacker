use crate::configuration::Settings;
use crate::db;
use crate::forms::StackPayload;
use crate::helpers::stack::builder::DcBuilder;
use crate::helpers::{JsonResponse, MqManager};
use crate::models;
use actix_web::{post, web, web::Data, Responder, Result};
use futures_lite::stream::StreamExt;
use lapin::{
    options::*, publisher_confirm::Confirmation, BasicProperties, Connection, ConnectionProperties,
};
use sqlx::PgPool;
use std::sync::Arc;

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
            Some(stack) => Ok(stack),
            None => Err(JsonResponse::<models::Stack>::build().not_found("not found")),
        })?;

    let id = stack.id.clone();
    let dc = DcBuilder::new(stack);
    dc.build().ok_or_else(|| {
        tracing::error!("Error. Compose builder returned an empty string");
        JsonResponse::<models::Stack>::build().internal_server_error("")
    })?;

    let mut stack_data =
        serde_json::from_value::<StackPayload>(dc.stack.body.clone()).map_err(|err| {
            tracing::error!("transforming json Value into StackPayload {:?}", err);
            JsonResponse::<models::Stack>::build().bad_request("")
        })?;

    stack_data.id = Some(id);
    stack_data.user_token = Some(user.id.clone());
    stack_data.user_email = Some(user.email.clone());
    stack_data.stack_code = stack_data.custom.custom_stack_code.clone();

    let payload = serde_json::to_string::<StackPayload>(&stack_data).map_err(|err| {
        tracing::error!("serializing StackPayload {:?}", err);
        JsonResponse::<models::Stack>::build().internal_server_error("")
    })?;

    let addr = sets.amqp.connection_string();
    let routing_key = "install.start.tfa.all.all".to_string();
    tracing::debug!("Sending message to {:?}", routing_key);

    Connection::connect(&addr, ConnectionProperties::default())
        .await
        .map_err(|err| {
            tracing::error!("connecting to RabbitMQ {:?}", err);
            JsonResponse::<models::Stack>::build().internal_server_error("")
        })?
        .create_channel()
        .await
        .map_err(|err| {
            tracing::error!("creating RabbitMQ channel {:?}", err);
            JsonResponse::<models::Stack>::build().internal_server_error("")
        })?
        .basic_publish(
            "install",
            routing_key.as_str(),
            BasicPublishOptions::default(),
            payload.as_bytes(),
            BasicProperties::default(),
        )
        .await
        .map_err(|err| {
            tracing::error!("publishing the message {:?}", err);
            JsonResponse::<models::Stack>::build().internal_server_error("")
        })?
        .await
        .map_err(|err| {
            tracing::error!("confirming the publication {:?}", err);
            JsonResponse::<models::Stack>::build().internal_server_error("")
        })
        .and_then(|confirm| match confirm {
            Confirmation::NotRequested => {
                Err(JsonResponse::<models::Stack>::build()
                    .bad_request("confirmation is NotRequested"))
            }
            _ => Ok(JsonResponse::<models::Stack>::build()
                .set_id(id)
                .ok("Success")),
        })
}
