use crate::configuration::Settings;
use crate::helpers::client;
use crate::helpers::JsonResponse;
use crate::models::user::User;
use crate::models::Client;
use actix_web::error::ErrorInternalServerError;
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

                return JsonResponse::build().bad_request("Too many clients created");
            }
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            return JsonResponse::build().internal_server_error("Internal Server Error");
        }
    };

    let mut client = Client::default();
    client.id = 1;
    client.user_id = user.id.clone();
    client.secret = client::generate_secret(pool.get_ref(), 255)
        .await
        .map(|s| Some(s))
        .map_err(|s| ErrorInternalServerError(s))?; //todo move to helpers::JsonResponse

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

            return JsonResponse::build()
                .set_id(client.id)
                .set_item(Some(client))
                .ok("OK");
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            let err = format!("Failed to insert. {}", e);
            return JsonResponse::build().bad_request(err);
        }
    }
}
