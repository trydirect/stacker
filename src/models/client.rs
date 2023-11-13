use serde::Serialize;

#[derive(Default, Serialize, Debug)]
pub struct Client {
    pub id: i32,
    pub user_id: String,
    pub secret: Option<String>, //todo hide secret
}
