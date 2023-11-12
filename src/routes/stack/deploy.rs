use std::sync::Arc;
use actix_web::{
    web,
    post,
    web::{Data, Json},
    Responder, Result,
};
use crate::models::user::User;
use crate::models::stack::Stack;
use sqlx::PgPool;
use lapin::{
    options::*, publisher_confirm::Confirmation, types::FieldTable, BasicProperties, Connection,
    ConnectionProperties
};
use crate::configuration::Settings;
use crate::helpers::JsonResponse;
use crate::helpers::stack::builder::DcBuilder;
use futures_lite::stream::StreamExt;
use serde::Serialize;
use crate::forms::{StackForm, StackPayload};


#[derive(Serialize, Debug, Clone)]
struct Payload {
    user_token: String,
    user_email: String,
    installation_id: String,
}


#[tracing::instrument(name = "Deploy.")]
#[post("/{id}/deploy")]
pub async fn add(
    user: web::ReqData<User>,
    path: web::Path<(i32,)>,
    pool: Data<PgPool>,
    sets: Data<Arc<Settings>>,
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
            let mut dc = DcBuilder::new(stack);
            dc.build();

            let addr = sets.amqp.connection_string();
            let routing_key = "install.start.tfa.all.all".to_string();
            tracing::debug!("Sending message to {:?}", routing_key);

            let conn = Connection::connect(&addr, ConnectionProperties::default())
                .await
                .unwrap();

            tracing::info!("RABBITMQ CONNECTED");

            let channel = conn.create_channel().await.unwrap();
            let mut stack_data = serde_json::from_value::<StackPayload>(
                dc.stack.body.clone()
            ).unwrap();

            stack_data.installation_id = Some(1);
            stack_data.user_token = Some(user.id.clone());
            stack_data.user_email= Some(user.email.clone());

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
                .await.unwrap()
                .await.unwrap();

            assert_eq!(confirm, Confirmation::NotRequested);
            tracing::debug!("Message sent to rabbitmq");

            Ok(Json(JsonResponse::<Stack>::new(
                "OK".to_owned(),
                "Success".to_owned(),
                200,
                Some(id),
                None,
                None
            )))
        }
        None => {
            Ok(Json(JsonResponse::internal_error("Deployment failed")))
        }
    }
}
