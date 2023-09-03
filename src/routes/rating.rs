use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};


// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[derive(Serialize, Deserialize, Debug)]
pub struct RatingForm {
    pub obj_id: u32,               // product external id
    pub category: String,          // rating of product | rating of service etc
    pub comment: String,           // always linked to a product
    pub rate: u32,                 //
}

pub async fn rating(form: web::Json<RatingForm>) -> HttpResponse {
    println!("{:?}", form);
    HttpResponse::Ok().finish()
}
