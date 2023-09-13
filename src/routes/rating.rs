use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use crate::models::rating::RateCategory;
use serde_valid::Validate;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;
use crate::startup::AppState;
use tracing::Instrument;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[derive(Serialize, Deserialize, Debug, Validate)]
pub struct RatingForm {
    pub obj_id: i32,                   // product external id
    pub category: RateCategory,        // rating of product | rating of service etc
    #[validate(max_length = 1000)]
    pub comment: Option<String>,       // always linked to a product
    #[validate(minimum = 0)]
    #[validate(maximum = 10)]
    pub rate: i32,                     //
}

pub async fn rating(app_state: web::Data<AppState>, form: web::Json<RatingForm>, pool:
web::Data<PgPool>) -> HttpResponse {
    let request_id = Uuid::new_v4();
    let user_id = app_state.user_id; // uuid Let's assume we have a user id already taken from auth


    let query_span = tracing::info_span!(
        "Saving new rating details in the database"
    );
    // Get product by id
    // Insert rating

    // match sqlx::query!(
    //     r#"
    //     INSERT INTO rating (user_id, product_id, category, comment, hidden,rate,
    //     created_at,
    //     updated_at)
    //     VALUES ($1, $2, $3, $4, $5, $6, NOW() at time zone 'utc', NOW() at time zone 'utc')
    //     "#,
    //     user_id,
    //     form.obj_id,
    //     form.category,
    //     form.comment,
    //     false,
    //     form.rate
    // )
    // .execute(pool.get_ref())
    // .instrument(query_span)
    // .await
    // {
    //     Ok(_) => {
    //         tracing::info!(
    //             "req_id: {} New subscriber details have been saved to database",
    //             request_id
    //         );
    //         HttpResponse::Ok().finish()
    //     }
    //     Err(e) => {
    //         tracing::error!("req_id: {} Failed to execute query: {:?}", request_id, e);
    //         HttpResponse::InternalServerError().finish()
    //     }
    // }
    println!("{:?}", form);
    HttpResponse::Ok().finish()
}
