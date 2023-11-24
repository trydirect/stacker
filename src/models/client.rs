use serde::Serialize;

#[derive(Default, Serialize)]
pub struct Client {
    pub id: i32,
    pub user_id: String,
    pub secret: Option<String>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let secret: String = match self.secret.as_ref() {
            Some(val) => val.chars().take(4).collect::<String>() + "****",
            None => "".to_string(),
        };

        write!(
            f,
            "Client {{id: {:?}, user_id: {:?}, secret: {}}}",
            self.id, self.user_id, secret
        )
    }
}
