use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct User {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub role: String,
    pub email_confirmed: bool,
    /// Access token used for proxy requests to other services (e.g., User Service)
    /// This is set during authentication and used for MCP tool calls.
    #[serde(skip)]
    pub access_token: Option<String>,
}

impl User {
    /// Create a new User with an access token for service proxy requests
    pub fn with_token(mut self, token: String) -> Self {
        self.access_token = Some(token);
        self
    }
}
