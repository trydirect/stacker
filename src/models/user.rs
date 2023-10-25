use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: String,
}

impl Clone for User {
    fn clone(&self) -> Self {
        User {
            id: self.id.clone()
        }
    }
}