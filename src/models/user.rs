use serde::Deserialize;

#[derive(Deserialize, Clone)]
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

impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("first_name", &self.first_name)
            .field("last_name", &self.last_name)
            .field("email", &self.email)
            .field("role", &self.role)
            .field("email_confirmed", &self.email_confirmed)
            .field("access_token", &"[REDACTED]")
            .finish()
    }
}
