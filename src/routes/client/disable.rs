use crate::configuration::Settings;
use crate::models::user::User;
use crate::models::Client;
use actix_web::{
    error::{ErrorForbidden, ErrorInternalServerError, ErrorNotFound},
    put, web, Responder, Result,
};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;

#[derive(Serialize)]
struct ClientDisableResponse {
    status: String,
    message: String,
    code: u32,
    client: Option<Client>,
}

#[tracing::instrument(name = "Disable client.")]
#[put("/{id}/disable")]
pub async fn disable_handler(
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
        Ok(_client) => Err(ErrorForbidden("client is not active")),
        Err(sqlx::Error::RowNotFound) => Err(ErrorNotFound("the client is not found")),
        Err(e) => {
            tracing::error!("Failed to execute fetch query: {:?}", e);

            Err(ErrorInternalServerError(""))
        }
    }?;

    client.secret = None;
    let query_span = tracing::info_span!("Updating client into the database");
    match sqlx::query!(
        r#"
        UPDATE client SET 
            secret=null,
            updated_at=NOW() at time zone 'utc'
        WHERE id = $1
        "#,
        client.id
    )
    .execute(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(_) => {
            tracing::info!("Client {} have been saved to database", client.id);
            Ok(web::Json(ClientDisableResponse {
                status: "success".to_string(),
                message: "".to_string(),
                code: 200,
                client: Some(client),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            return Err(ErrorInternalServerError(""));
        }
    }
}
