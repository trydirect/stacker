use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub email_confirmed: bool,
    // pub phone: Option<String>,
    // pub website: Option<String>,
}
