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

impl Clone for User {
    fn clone(&self) -> Self {
        User {
            id: self.id.clone(),
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            email: self.email.clone(),
            email_confirmed: self.email_confirmed.clone(),
        }
    }
}