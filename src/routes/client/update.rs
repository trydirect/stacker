use crate::configuration::Settings;
use crate::helpers::client;
use crate::models::user::User;
use crate::models::Client;
use actix_web::{error::ErrorNotFound, put, web, HttpResponse, Responder, Result};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;

#[derive(Serialize)]
struct ClientUpdateResponse {
    status: String,
    message: String,
    code: u32,
    client: Option<Client>,
}

#[tracing::instrument(name = "Update client.")]
#[put("/{id}")]
pub async fn update_handler(
    user: web::ReqData<User>,
    settings: web::Data<Arc<Settings>>,
    pool: web::Data<PgPool>,
    path: web::Path<(i32,)>,
) -> Result<impl Responder> {
    let client_id = path.0;
    //todo 1. find the client
    //todo 2. if client is disabled. I mean the client_secret is null no action is to be performed
    //todo 3. if client is active. update the secret and the updated_at fields
    let query_span = tracing::info_span!("Fetching the client by ID");
    let client: Client = match sqlx::query_as!(
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
        Ok(client) => Ok(client), //todo continue only if not null secret
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);

            /*
            return Ok(web::Json(ClientUpdateResponse {
                status: "error".to_string(),
                code: 500,
                message: "Failed to insert".to_string(),
                client: None,
            }));
            */
            Err(ErrorNotFound("the client is not found")) //todo add a correct message
        }
    }?;

    /*
    let mut client = Client::default();
    client.id = 1;
    client.user_id = user.id.clone();
    client.secret = loop {
        let secret = client::generate_secret(255);
        match client::is_secret_unique(pool.get_ref(), &secret).await {
            Ok(is_unique) if is_unique => {
                break secret;
            }
            Ok(_) => {
                tracing::info!("Generate secret once more.");
                continue;
            }
            Err(e) => {
                tracing::error!("Failed to execute query: {:?}", e);

                return Ok(web::Json(ClientAddResponse {
                    status: "error".to_string(),
                    code: 500,
                    message: "Failed to insert".to_string(),
                    client: None,
                }));
            }
        }
    };

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
            Ok(web::Json(ClientAddResponse {
                status: "success".to_string(),
                message: "".to_string(),
                code: 200,
                client: Some(client),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);

            return Ok(web::Json(ClientAddResponse {
                status: "error".to_string(),
                code: 500,
                message: "Failed to insert".to_string(),
                client: None,
            }));
        }
    }
    */

    return Ok(web::Json(ClientUpdateResponse {
        status: format!("client_id={}", path.0),
        code: 200,
        message: "Failed to update".to_string(),
        client: Some(client),
    }));
}
