use uuid::Uuid;
use chrono::{DateTime, Utc};

pub struct Product {
    // Product - is an external object that we want to store in the database,
    // that can be a stack or an app in the stack. feature, service, web app etc.
    // id - is a unique identifier for the product
    // user_id - is a unique identifier for the user
    // rating - is a rating of the product
    // product type stack & app,
    // id is generated based on the product type and external obj_id
    pub id: i32,                   //primary key, for better data management
    pub obj_id: u32,               // external product ID db, no autoincrement, example: 100
    pub obj_type: String,          // stack | app, unique index
    pub rating: Rating,               // 0-10
    // pub rules: Rules,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Rating {
    pub id: i32,
    pub user_id: Uuid,             // external user_id, 100, taken using token (middleware?)
    pub category: String,
    pub comment: String,           // always linked to a product
    pub hidden: bool,              // rating can be hidden for non-adequate user behaviour
    pub rate: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Rules {
    //-> Product.id
    // example: allow to add only a single comment
    comments_per_user: i32, // default = 1
}

