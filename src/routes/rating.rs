use actix_web::HttpResponse;
use uuid::Uuid;
use chrono::{DateTime, Utc};

pub struct Product {
    // Product - is an external object that we want to store in the database,
    // that can be a stack or an app in the stack. feature, service, web app etc.
    // id - is a unique identifier for the product
    // user_id - is a unique identifier for the user
    // rating - is a rating of the product
    pub id: i32,      // internal database primary key
    pub obj_id: Uuid, //
    pub user_id: Uuid,
    pub rating: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}


pub async fn rating() -> HttpResponse {
    unimplemented!()
}
