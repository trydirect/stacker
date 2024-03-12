use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use chrono::{DateTime, Utc};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Cloud {
    pub user_id: String,
    pub project_id: Option<i32>,
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
        cloud.project_id = self.project_id;
        cloud.provider = self.provider;
        cloud.cloud_token = self.cloud_token;
        cloud.cloud_key = self.cloud_key;
        cloud.cloud_secret = self.cloud_secret;
        cloud.save_token = self.save_token;
        cloud.created_at = Utc::now();
        cloud.updated_at = Utc::now();

        cloud
    }
}

impl Into<Cloud> for models::Cloud {
    fn into(self) -> Cloud {
        let mut form = Cloud::default();
        form.user_id = self.user_id;
        form.project_id = self.project_id;
        form.provider = self.provider;
        form.cloud_token = self.cloud_token;
        form.cloud_key = self.cloud_key;
        form.cloud_secret = self.cloud_secret;
        form.save_token = self.save_token;

        form
    }
}
