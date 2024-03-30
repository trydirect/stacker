use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

fn mask_string(s: Option<&String>) -> String {
    match s {
        Some(val) => val.chars().take(4).collect::<String>() + "****",
        None => "".to_string(),
    }
}

impl std::fmt::Display for Cloud {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cloud_key = mask_string(self.cloud_key.as_ref());
        let cloud_token = mask_string(self.cloud_token.as_ref());
        let cloud_secret = mask_string(self.cloud_secret.as_ref());

        write!(f, "{} cloud creds: cloud_key : {} cloud_token: {} cloud_secret: {} project_id: {:?}",
               self.provider,
               cloud_key,
               cloud_token,
               cloud_secret,
               self.project_id
        )
    }
}

impl Cloud {
    pub fn new(user_id: String,
               project_id: Option<i32>,
               provider: String,
               cloud_token: Option<String>,
               cloud_key: Option<String>,
               cloud_secret: Option<String>,
               save_token: Option<bool>
    ) -> Self {
        Self {
            id: 0,
            user_id,
            project_id,
            provider,
            cloud_token,
            cloud_key,
            cloud_secret,
            save_token,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl Default for Cloud {
    fn default() -> Self {
        Cloud {
            id: 0,
            provider: "".to_string(),
            user_id: "".to_string(),
            project_id: Default::default(),
            cloud_key: Default::default(),
            cloud_token: Default::default(),
            cloud_secret: Default::default(),
            save_token: Some(false),
            created_at: Default::default(),
            updated_at: Default::default(),
        }
    }
}
