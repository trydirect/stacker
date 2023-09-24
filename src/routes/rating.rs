use crate::forms;
use crate::models;
use crate::startup::AppState;
use actix_web::{web, HttpResponse, Responder, Result};
use serde_derive::Serialize;
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;
use crate::models::RateCategory;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[derive(Serialize)]
struct JsonResponse {
    status: String,
    message: String,
    code: u32,
    id: Option<i32>
}

pub async fn rating(
    app_state: web::Data<AppState>,
    form: web::Json<forms::Rating>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    //TODO. check if there already exists a rating for this product committed by this user
    let request_id = Uuid::new_v4();
    let query_span = tracing::info_span!("Check product existence by id.");
    match sqlx::query_as!(
        models::Product,
        r"SELECT * FROM product WHERE obj_id = $1",
        form.obj_id
    )
    .fetch_one(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(product) => {
            tracing::info!("req_id: {} Found product: {:?}", request_id, product.obj_id);
        }
        Err(e) => {
            tracing::error!(
                "req_id: {} Failed to fetch product: {:?}, error: {:?}",
                request_id,
                form.obj_id,
                e
            );
            // return HttpResponse::InternalServerError().finish();
            return Ok(web::Json(JsonResponse {
                status : "Error".to_string(),
                code: 404,
                message: format!("Object not found {}", form.obj_id),
                id: None
            }));
        }
    };

    let user_id = app_state.user_id; // uuid Let's assume user_id already taken from auth

    let query_span = tracing::info_span!("Search for existing vote.");
    match sqlx::query!(
        r"SELECT id FROM rating where user_id=$1 AND product_id=$2 AND category=$3 LIMIT 1",
        user_id,
        form.obj_id,
        form.category as RateCategory
    )
        .fetch_one(pool.get_ref())
        .instrument(query_span)
        .await
    {
        Ok(record) =>  {
            tracing::info!("req_id: {} rating exists: {:?}, user: {}, product: {}, category: {:?}",
                request_id, record.id, user_id, form.obj_id, form.category);

            return Ok(web::Json(JsonResponse{
                status: "Error".to_string(),
                code: 409,
                message: format!("Already Rated"),
                id: Some(record.id)
            }));
        }
        Err(err) => {
          // @todo, match the sqlx response
        }
    }

    let query_span = tracing::info_span!("Saving new rating details into the database");
    // Get product by id
    // Insert rating
    match sqlx::query!(
        r#"
        INSERT INTO rating (user_id, product_id, category, comment, hidden,rate,
        created_at,
        updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW() at time zone 'utc', NOW() at time zone 'utc')
        RETURNING id
        "#,
        user_id,
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
            println!("Query returned {:?}", result);
            //TODO return json containing the id of the new rating
            tracing::info!(
                "req_id: {} New rating {} have been saved to database",
                request_id,
                result.id
            );

            Ok(web::Json(JsonResponse {
                status : "ok".to_string(),
                code: 200,
                message: "Saved".to_string(),
                id: Some(result.id)
            }))
        }
        Err(e) => {
            tracing::error!("req_id: {} Failed to execute query: {:?}", request_id, e);
           Ok(web::Json(JsonResponse{
               status: "error".to_string(),
               code: 500,
               message: "Failed to insert".to_string(),
               id: None
           }))
        }
    }
}
