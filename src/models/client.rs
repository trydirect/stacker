use serde::Serialize;

#[derive(Default, Serialize)]
pub struct Client {
    pub id: i32,
    pub user_id: String,
    pub secret: Option<String>,
}
//todo add created_at AND updated_at fields
