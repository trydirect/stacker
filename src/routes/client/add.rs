use crate::configuration::Settings;
use crate::helpers::client;
use crate::helpers::JsonResponse;
use crate::models;
use crate::models::user::User;
use actix_web::{post, web, Responder, Result};
use sqlx::PgPool;
use tracing::Instrument;
use std::sync::Arc;

#[tracing::instrument(name = "Add client.")]
#[post("")]
pub async fn add_handler(
    user: web::ReqData<Arc<User>>,
    settings: web::Data<Settings>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    match add_handler_inner(&user.id, settings, pool).await {
        Ok(client) => JsonResponse::build().set_item(client).ok("Ok"),
        Err(msg) => JsonResponse::build().bad_request(msg),
    }
}

pub async fn add_handler_inner(
    user_id: &String,
    settings: web::Data<Settings>,
    pool: web::Data<PgPool>,
) -> Result<models::Client, String> {
    let query_span = tracing::info_span!("Counting the user's clients");

    match sqlx::query!(
        r#"
        SELECT
            count(*) as client_count
        FROM client c 
        WHERE c.user_id = $1
        "#,
        user_id.clone(),
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(result) => {
            let client_count = result.client_count.unwrap();
            if client_count >= settings.max_clients_number {
                tracing::error!(
                    "Too many clients. The user {} has {} clients",
                    user_id,
                    client_count
                );

                return Err("Too many clients created".to_string());
            }
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            return Err("Internal Server Error".to_string());
        }
    };

    let mut client = models::Client::default();
    client.user_id = user_id.clone();
    client.secret = client::generate_secret(pool.get_ref(), 255)
        .await
        .map(|s| Some(s))?;

    let query_span = tracing::info_span!("Saving new client into the database");
    match sqlx::query!(
        r#"
        INSERT INTO client (user_id, secret, created_at, updated_at)
        VALUES ($1, $2, NOW() at time zone 'utc', NOW() at time zone 'utc')
        RETURNING id
        "#,
        client.user_id.clone(),
        client.secret,
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(result) => {
            tracing::info!("New client {} have been saved to database", result.id);
            client.id = result.id;

            return Ok(client);
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            return Err("Failed to insert".to_string());
        }
    }
}
