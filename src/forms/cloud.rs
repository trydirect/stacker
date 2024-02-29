use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize, Debug, Validate)]
pub struct Cloud {
    pub user_id: String,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub provider: String,
    pub cloud_token: Option<String>,
    pub cloud_key: Option<String>,
    pub cloud_secret: Option<String>,
    pub save_token: Option<bool>,
}

impl Into<models::Cloud> for Cloud {
    fn into(self) -> models::Cloud {
        let mut cloud = models::Cloud::default();
        cloud.user_id = self.user_id;
        cloud.provider = self.provider;
        cloud.cloud_token = Some(String::from(""));
        cloud.cloud_key = Some(String::from(""));
        cloud.cloud_secret = Some(String::from(""));
        cloud.created_at = Utc::now();
        cloud.updated_at = Utc::now();

        cloud
    }
}
