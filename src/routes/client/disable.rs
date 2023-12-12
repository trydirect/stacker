use crate::configuration::Settings;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{put, web, Responder, Result};
use sqlx::PgPool;
use tracing::Instrument;
use std::sync::Arc;

#[tracing::instrument(name = "Disable client.")]
#[put("/{id}/disable")]
pub async fn disable_handler(
    user: web::ReqData<Arc<models::User>>,
    settings: web::Data<Settings>,
    pool: web::Data<PgPool>,
    path: web::Path<(i32,)>,
) -> Result<impl Responder> {
        let client_id = path.0;
        let mut client = db_fetch_client_by_id(pool.get_ref(), client_id)
            .await
            .map_err(|msg| JsonResponse::<models::Client>::build().not_found(msg))
            ?; 
        if client.secret.is_none() {
            return Err(JsonResponse::<models::Client>::build().bad_request("client is not active"));
        }

        client.secret = None;
        let client = db_update_client(pool.get_ref(), client)
            .await
            .map_err(|msg| JsonResponse::<models::Client>::build().bad_request(msg))?;

        Ok(JsonResponse::build().set_item(client).ok("success"))
}

async fn db_fetch_client_by_id(pool: &PgPool, id: i32) -> Result<models::Client, String> {
    let query_span = tracing::info_span!("Fetching the client by ID");
    sqlx::query_as!(
        models::Client,
        r#"
        SELECT
           id, user_id, secret 
        FROM client c
        WHERE c.id = $1
        "#,
        id,
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        match e {
            sqlx::Error::RowNotFound => "client not found".to_string(),
            s => {
                tracing::error!("Failed to execute fetch query: {:?}", s);
                "".to_string()
            }
        }
    })
}

async fn db_update_client(pool: &PgPool, client: models::Client) -> Result<models::Client , String> {
    let query_span = tracing::info_span!("Updating client into the database");
    sqlx::query!(
        r#"
        UPDATE client SET 
            secret=$1,
            updated_at=NOW() at time zone 'utc'
        WHERE id = $2
        "#,
        client.secret,
        client.id
    )
    .execute(pool)
    .instrument(query_span)
    .await
    .map(|_|{
        tracing::info!("Client {} have been saved to database", client.id);
        client
    })
    .map_err(|err| {
        tracing::error!("Failed to execute query: {:?}", err);
        "".to_string()
    })
}
