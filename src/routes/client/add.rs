use crate::configuration::Settings;
use crate::helpers::{client, JsonResponse};
use crate::models::user::User;
use crate::models::Client;
use actix_web::{post, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;


#[tracing::instrument(name = "Add client.")]
#[post("")]
pub async fn add_handler(
    user: web::ReqData<User>,
    settings: web::Data<Arc<Settings>>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let query_span = tracing::info_span!("Counting the user's clients");

    match sqlx::query!(
        r#"
        SELECT
            count(*) as client_count
        FROM client c 
        WHERE c.user_id = $1
        "#,
        user.id.clone(),
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
                    user.id,
                    client_count
                );

                return Ok(web::Json(JsonResponse::not_valid(
                    "Too many clients already created"))
                );
            }
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            return Ok(web::Json(JsonResponse::internal_error("Failed to insert")));
        }
    };

    let mut client = Client::default();
    client.id = 1;
    client.user_id = user.id.clone();
    client.secret = client::generate_secret(255);

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
            Ok(web::Json(JsonResponse::new(
                "success".to_string(),
                "".to_string(),
                200,
                Some(client.id),
                Some(client),
                None
            )))
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            Ok(web::Json(JsonResponse::internal_error("Failed to insert")))
        }
    }
}
