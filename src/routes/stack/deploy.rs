use crate::configuration::Settings;
use crate::db;
use crate::forms::StackPayload;
use crate::helpers::stack::builder::DcBuilder;
use crate::helpers::JsonResponse;
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
    pool: Data<PgPool>,
    sets: Data<Settings>,
) -> Result<impl Responder> {
    let id = path.0;
    let stack = db::stack::fetch(pool.get_ref(), id)
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
        JsonResponse::<models::Stack>::build().internal_server_error("troubles at building")
    })?;

    let addr = sets.amqp.connection_string();
    let routing_key = "install.start.tfa.all.all".to_string();
    tracing::debug!("Sending message to {:?}", routing_key);

    let conn = Connection::connect(&addr, ConnectionProperties::default())
        .await
        .map_err(|err| {
            JsonResponse::<models::Stack>::build()
                .internal_server_error("Could not connect RabbitMQ")
        })?;

    let channel = conn.create_channel().await.map_err(|err| {
        JsonResponse::<models::Stack>::build()
            .internal_server_error("Can't create rabbitMQ channel")
    })?;
    let mut stack_data = serde_json::from_value::<StackPayload>(dc.stack.body.clone())
        .map_err(|err| JsonResponse::<models::Stack>::build().bad_request("can't deserialize"))?;

    stack_data.id = Some(id);
    stack_data.user_token = Some(user.id.clone());
    stack_data.user_email = Some(user.email.clone());
    stack_data.stack_code = stack_data.custom.custom_stack_code.clone();

    let payload = serde_json::to_string::<StackPayload>(&stack_data).map_err(|err| {
        JsonResponse::<models::Stack>::build().internal_server_error(format!("{}", err))
    })?;

    channel
        .basic_publish(
            "install",
            routing_key.as_str(),
            BasicPublishOptions::default(),
            payload.as_bytes(),
            BasicProperties::default(),
        )
        .await
        .map_err(|_| {
            JsonResponse::<models::Stack>::build().internal_server_error("internal server error")
        })? //todo the correct err
        .await
        .map_err(|_| {
            JsonResponse::<models::Stack>::build().internal_server_error("internal server error")
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
