use crate::models::user::User;
use crate::models::Client;
use actix_web::{post, web, Responder, Result};
use rand::Rng;
use serde::Serialize;
use sqlx::PgPool;
use tracing::Instrument;

#[derive(Serialize)]
struct ClientAddResponse {
    status: String,
    message: String,
    code: u32,
    client: Option<Client>,
}

fn generate_secret(len: usize) -> String {
    const CHARSET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789)(*&^%$#@!~";
    let mut rng = rand::thread_rng();

    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[tracing::instrument(name = "Add client.")]
#[post("")]
pub async fn add_handler(
    user: web::ReqData<User>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    //todo how many clients can an user have?
    let mut client = Client::default();
    client.id = 1;
    client.user_id = user.id.clone();
    client.secret = generate_secret(255);
    //todo 3. update entity with the database's generated id
    //todo 4. it throws 500 when the AS is not reachable. it should just return 401

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
}
