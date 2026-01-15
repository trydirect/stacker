use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct User {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub role: String,
    pub email_confirmed: bool,
}
