use crate::configuration::Settings;
use crate::forms::StackPayload;
use crate::helpers::stack::builder::DcBuilder;
use crate::helpers::JsonResponse;
use crate::models::stack::Stack;
use crate::models::user::User;
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
    user: web::ReqData<Arc<User>>,
    path: web::Path<(i32,)>,
    pool: Data<PgPool>,
    sets: Data<Settings>,
) -> Result<impl Responder> {
    let id = path.0;
    tracing::debug!("Received id: {}", id);

    let stack = match sqlx::query_as!(
        Stack,
        r#"
        SELECT * FROM user_stack WHERE id=$1 LIMIT 1
        "#,
        id
    )
    .fetch_one(pool.get_ref())
    .await
    {
        Ok(stack) => {
            tracing::info!("Stack found: {:?}", stack.id,);
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

    return match stack {
        Some(stack) => {
            let id = stack.id.clone();
            let dc = DcBuilder::new(stack);
            dc.build();

            let addr = sets.amqp.connection_string();
            let routing_key = "install.start.tfa.all.all".to_string();
            tracing::debug!("Sending message to {:?}", routing_key);

            let conn = Connection::connect(&addr, ConnectionProperties::default())
                .await
                .expect("Could not connect RabbitMQ");

            tracing::info!("RABBITMQ CONNECTED");

            let channel = conn.create_channel().await.unwrap();
            let mut stack_data =
                serde_json::from_value::<StackPayload>(dc.stack.body.clone()).unwrap();

            stack_data.id = Some(id);
            stack_data.user_token = Some(user.id.clone());
            stack_data.user_email = Some(user.email.clone());
            stack_data.stack_code = stack_data.custom.custom_stack_code.clone();

            let payload = serde_json::to_string::<StackPayload>(&stack_data).unwrap();
            let _payload = payload.as_bytes();

            let confirm = channel
                .basic_publish(
                    "install",
                    routing_key.as_str(),
                    BasicPublishOptions::default(),
                    _payload,
                    BasicProperties::default(),
                )
                .await
                .unwrap()
                .await
                .unwrap();

            assert_eq!(confirm, Confirmation::NotRequested);
            tracing::debug!("Message sent to rabbitmq");
            return JsonResponse::<Stack>::build().set_id(id).ok("Success");
        }
        None => {
            JsonResponse::build().internal_server_error("Deployment failed")
        }
    }
}
