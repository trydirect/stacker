use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use crate::models::rating::RateCategory;
use serde_valid::Validate;
use sqlx::PgPool;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[derive(Serialize, Deserialize, Debug, Validate)]
pub struct RatingForm {
    pub obj_id: u32,               // product external id
    pub category: RateCategory,    // rating of product | rating of service etc
    #[validate(max_length = 1000)]
    pub comment: Option<String>,           // always linked to a product
    #[validate(minimum = 0)]
    #[validate(maximum = 10)]
    pub rate: u32,                 //
}

pub async fn rating(form: web::Json<RatingForm>, pool: web::Data<PgPool>) -> HttpResponse {
    let user_id = 1; // Let's assume we have a user id already taken from auth

    // Get product by id
    // Insert rating

    // match sqlx::query!(
    //     r#"
    //     INSERT INTO rating ()
    //     VALUES ($1, $2, $3, $4)
    //     "#,
    //     args
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
