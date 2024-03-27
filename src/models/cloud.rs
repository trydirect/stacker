use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cloud {
    pub id: i32,
    pub user_id: String,
    pub project_id: Option<i32>,
    pub provider: String,
    pub cloud_token: Option<String>,
    pub cloud_key: Option<String>,
    pub cloud_secret: Option<String>,
    pub save_token: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}



impl std::fmt::Display for Cloud {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cloud_key: String = match self.cloud_key.as_ref() {
            Some(val) =>
                {
                    val.chars().take(4).collect::<String>() + "****"
                },
            None => "".to_string(),
        };
        let cloud_token: String = match self.cloud_token.as_ref() {
            Some(val) => {
                val.chars().take(4).collect::<String>() + "****"
            },
            None => "".to_string(),
        };

        let cloud_secret: String = match self.cloud_secret.as_ref() {
            Some(val) => {
                val.chars().take(4).collect::<String>() + "****"
            }
            None => "".to_string(),
        };

        write!(f, "{} cloud creds: cloud_key : {} cloud_token: {} cloud_secret: {} project_id: {:?}",
               self.provider,
               cloud_key,
               cloud_token,
               cloud_secret,
               self.project_id
        )
    }
}
