use crate::models::user::User;
use crate::models::Client;
use actix_web::{post, web, Responder, Result};
use serde::Serialize;
use sqlx::PgPool;

#[derive(Serialize)]
struct ClientAddResponse {
    status: String,
    code: u32,
    client: Client,
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
    client.secret = "secret".to_string();
    //todo 1. genereate random secret. 255symbols
    //todo 2. save it to database
    //todo 3. update entity with the database's generated id
    //todo 4. it throws 500 when the AS is not reachable. it should just return 401

    /*
    let query_span = tracing::info_span!("Saving new rating details into the database");
    // Insert rating
    match sqlx::query!(
        r#"
        INSERT INTO rating (user_id, product_id, category, comment, hidden,rate,
        created_at,
        updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW() at time zone 'utc', NOW() at time zone 'utc')
        RETURNING id
        "#,
        user.id,
        form.obj_id,
        form.category as models::RateCategory,
        form.comment,
        false,
        form.rate
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(result) => {
            tracing::info!("New rating {} have been saved to database", result.id);

            Ok(web::Json(JsonResponse {
                status: "ok".to_string(),
                code: 200,
                message: "Saved".to_string(),
                id: Some(result.id),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            Ok(web::Json(JsonResponse {
                status: "error".to_string(),
                code: 500,
                message: "Failed to insert".to_string(),
                id: None,
            }))
        }
    }
    */
    //save client in DB
    //return the client as a response
    return Ok(web::Json(ClientAddResponse {
        status: "success".to_string(),
        code: 200,
        client: client,
    }));
}
