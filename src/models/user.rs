use serde::Deserialize;

#[derive(Debug, Copy, Clone, Deserialize)]
pub struct User {
    pub id: i32,
}
