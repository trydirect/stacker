use crate::configuration::Settings;
use crate::helpers::client;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{put, web, Responder, Result};
use sqlx::PgPool;
use tracing::Instrument;
use std::sync::Arc;

#[tracing::instrument(name = "Enable client.")]
#[put("/{id}/enable")]
pub async fn enable_handler(
    user: web::ReqData<Arc<models::User>>,
    settings: web::Data<Settings>,
    pool: web::Data<PgPool>,
    path: web::Path<(i32,)>,
) -> Result<impl Responder> {
    match async {
        let client_id = path.0;
        let mut client = db_fetch_client_by_id(pool.get_ref(), client_id).await?; 
        if client.secret.is_some() {
            return Err("client is already active".to_string());
        }

        client.secret = Some(client::generate_secret(pool.get_ref(), 255).await?);
        db_update_client(pool.get_ref(), client).await
    }.await {
        Ok(client) => {
            JsonResponse::build().set_item(client).ok("success")
        }
        Err(msg) => {
            JsonResponse::<models::Client>::build().bad_request(msg)
        }
    }
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

async fn db_update_client(pool: &PgPool, client: models::Client) -> Result<models::Client, String> {
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
