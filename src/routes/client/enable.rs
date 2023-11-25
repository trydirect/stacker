use crate::configuration::Settings;
use crate::helpers::client;
use crate::helpers::JsonResponse;
use crate::models::user::User;
use crate::models::Client;
use actix_web::{error::ErrorBadRequest, put, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;

#[tracing::instrument(name = "Enable client.")]
#[put("/{id}/enable")]
pub async fn enable_handler(
    user: web::ReqData<User>,
    settings: web::Data<Arc<Settings>>,
    pool: web::Data<PgPool>,
    path: web::Path<(i32,)>,
) -> Result<impl Responder> {
    let client_id = path.0;
    let query_span = tracing::info_span!("Fetching the client by ID");
    let mut client: Client = match sqlx::query_as!(
        Client,
        r#"
        SELECT
           id, user_id, secret 
        FROM client c
        WHERE c.id = $1
        "#,
        client_id,
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(client) if client.secret.is_none() => Ok(client),
        Ok(_client) => Err("client is already enabled"),
        Err(sqlx::Error::RowNotFound) => Err("the client is not found"),
        Err(e) => {
            tracing::error!("Failed to execute fetch query: {:?}", e);

            Err("")
        }
    }
    .map_err(|s| ErrorBadRequest(JsonResponse::<Client>::build().set_msg(s).to_string()))?;

    client.secret = client::generate_secret(pool.get_ref(), 255)
        .await
        .map(|s| Some(s))
        .map_err(|s| ErrorBadRequest(s))?;

    let query_span = tracing::info_span!("Updating client into the database");
    match sqlx::query!(
        r#"
        UPDATE client SET 
            secret=$1,
            updated_at=NOW() at time zone 'utc'
        WHERE id = $2
        "#,
        client.secret,
        client.id
    )
    .execute(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(_) => {
            tracing::info!("Client {} have been saved to database", client.id);
            JsonResponse::build().set_item(client).ok("success")
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            JsonResponse::build().internal_server_error("")
        }
    }
}
