use crate::helpers::client;
use crate::models::user::User;
use crate::models::Client;
use crate::{configuration::Settings, helpers::JsonResponse};
use actix_web::{error::ErrorBadRequest, put, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;

#[tracing::instrument(name = "Update client.")]
#[put("/{id}")]
pub async fn update_handler(
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
        Ok(client) if client.secret.is_some() => Ok(client),
        Ok(_client) => Err("client is not active"),
        Err(sqlx::Error::RowNotFound) => Err("the client is not found"),
        Err(e) => {
            tracing::error!("Failed to execute fetch query: {:?}", e);

            Err("")
        }
    }
    .map_err(|s| ErrorBadRequest(s))?; //todo

    client.secret = client::generate_secret(pool.get_ref(), 255)
        .await
        .map(|s| Some(s))
        .map_err(|s| ErrorBadRequest(s))?; //todo

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
            JsonResponse::build().err_internal_server_error("")
        }
    }
}
