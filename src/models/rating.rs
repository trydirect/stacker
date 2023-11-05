use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub struct Product {
    // Product - is an external object that we want to store in the database,
    // that can be a stack or an app in the stack. feature, service, web app etc.
    // id - is a unique identifier for the product
    // user_id - is a unique identifier for the user
    // rating - is a rating of the product
    // product type stack & app,
    // id is generated based on the product type and external obj_id
    pub id: i32,          //primary key, for better data management
    pub obj_id: i32,      // external product ID db, no autoincrement, example: 100
    pub obj_type: String, // stack | app, unique index
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct Rating {
    pub id: i32,
    pub user_id: String, // external user_id, 100, taken using token (middleware?)
    pub obj_id: i32,     // id of the external object
    pub category: String, // rating of product | rating of service etc
    pub comment: Option<String>, // always linked to a product
    pub hidden: Option<bool>, // rating can be hidden for non-adequate user behaviour
    pub rate: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::Type, Serialize, Deserialize, Debug, Clone, Copy)]
#[sqlx(rename_all = "lowercase", type_name = "varchar")]
pub enum RateCategory {
    Application, // app, feature, extension
    Cloud,       // is user satisfied working with this cloud
    Stack,       // app stack
    DeploymentSpeed,
    Documentation,
    Design,
    TechSupport,
    Price,
    MemoryUsage,
}

impl Into<String> for RateCategory {
    fn into(self) -> String {
        format!("{:?}", self)
    }
}

pub struct Rules {
    //-> Product.id
    // example: allow to add only a single comment
    comments_per_user: i32, // default = 1
}
