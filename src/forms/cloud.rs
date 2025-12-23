use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use crate::helpers::cloud::security::Secret;
use chrono::Utc;


fn hide_parts(value: String) -> String {
    value.chars().into_iter().take(6).collect::<String>() + "****"
}

#[derive(Default, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct CloudForm {
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<i32>,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub provider: String,
    pub cloud_token: Option<String>,
    pub cloud_key: Option<String>,
    pub cloud_secret: Option<String>,
    pub save_token: Option<bool>,
}

impl CloudForm {
    #[tracing::instrument(name = "impl CloudForm::decode()")]
    pub(crate) fn decode(secret: &mut Secret, encrypted_value: String) -> String {
        // tracing::error!("encrypted_value {:?}", &encrypted_value);
        let b64_decoded = Secret::b64_decode(&encrypted_value).unwrap();
        // tracing::error!("decoded {:?}", &b64_decoded);
        match secret.decrypt(b64_decoded) {
            Ok(decoded) => decoded,
            Err(_err) => {
                tracing::error!("🟥 Could not decode {:?},{:?}",secret.field,_err);
                // panic!("Could not decode ");
                "".to_owned()
            }
        }
    }

    pub(crate) fn decrypt_field(
        secret: &mut Secret,
        field_name: &str,
        encrypted_value: Option<String>,
        reveal: bool,
    ) -> Option<String> {
        if let Some(val) = encrypted_value {
            secret.field = field_name.to_owned();
            let decoded_value = CloudForm::decode(secret, val);
            if reveal {
                return Some(decoded_value);
            } else {
                return Some(hide_parts(decoded_value));
            }
        }
        None
    }

    // @todo should be refactored, may be moved to cloud.into() or Secret::from()
    #[tracing::instrument(name = "decode_model")]
    pub fn decode_model(mut cloud: models::Cloud, reveal:bool) -> models::Cloud {

        let mut secret = Secret::new();
        secret.user_id = cloud.user_id.clone();
        secret.provider = cloud.provider.clone();
        cloud.cloud_token = CloudForm::decrypt_field(&mut secret, "cloud_token", cloud.cloud_token.clone(), reveal);
        cloud.cloud_secret = CloudForm::decrypt_field(&mut secret, "cloud_secret", cloud.cloud_secret.clone(), reveal);
        cloud.cloud_key = CloudForm::decrypt_field(&mut secret, "cloud_key", cloud.cloud_key.clone(), reveal);

        cloud
    }
}

impl std::fmt::Debug for CloudForm {
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
                eprintln!("cloud token {val:?}");
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

fn encrypt_field(
    secret: &mut Secret,
    field_name: &str,
    value: Option<String>,
) -> Option<String> {
    if let Some(val) = value {
        secret.field = field_name.to_owned();
        if let Ok(encrypted) = secret.encrypt(val) {
            return Some(Secret::b64_encode(&encrypted));
        }
    }
    None
}

impl Into<models::Cloud> for &CloudForm {
    #[tracing::instrument(name = "impl Into<models::Cloud> for &CloudForm")]
    fn into(self) -> models::Cloud {
        let mut cloud = models::Cloud::default();
        cloud.provider = self.provider.clone();
        cloud.user_id = self.user_id.clone().unwrap();

        if Some(true) == self.save_token {
            let mut secret = Secret::new();
            secret.user_id = self.user_id.clone().unwrap();
            secret.provider = self.provider.clone();

            cloud.cloud_token = encrypt_field(&mut secret, "cloud_token", self.cloud_token.clone());
            cloud.cloud_key = encrypt_field(&mut secret, "cloud_key", self.cloud_key.clone());
            cloud.cloud_secret = encrypt_field(&mut secret, "cloud_secret", self.cloud_secret.clone());
        } else {
            cloud.cloud_token = self.cloud_token.clone();
            cloud.cloud_key = self.cloud_key.clone();
            cloud.cloud_secret = self.cloud_secret.clone();
        }
        cloud.save_token = self.save_token.clone();
        cloud.created_at = Utc::now();
        cloud.updated_at = Utc::now();
        cloud
    }

}


// on deploy
impl Into<CloudForm> for models::Cloud {
    #[tracing::instrument(name = "Into<CloudForm> for models::Cloud .")]
    fn into(self) -> CloudForm {
        let mut form = CloudForm::default();
        form.provider = self.provider.clone();

        if Some(true) == self.save_token {
            let mut secret = Secret::new();
            secret.user_id = self.user_id.clone();
            secret.provider = self.provider;
            secret.field = "cloud_token".to_string();

            let value = match self.cloud_token {
                Some(value) => {
                    CloudForm::decode(&mut secret, value)
                }
                None => {
                    tracing::debug!("Skip {}", secret.field);
                    "".to_string()
                }
            };
            form.cloud_token = Some(value);

            secret.field = "cloud_key".to_string();
            let value = match self.cloud_key {
                Some(value) => {
                    CloudForm::decode(&mut secret, value)
                }
                None => {
                    tracing::debug!("Skipp {}", secret.field);
                    "".to_string()
                }
            };
            form.cloud_key = Some(value);

            secret.field = "cloud_secret".to_string();
            let value = match self.cloud_secret {
                Some(value) => {
                    CloudForm::decode(&mut secret, value)
                }
                None => {
                    tracing::debug!("Skipp {}", secret.field);
                    "".to_string()
                }
            };
            form.cloud_secret = Some(value);

        } else {
            form.cloud_token = self.cloud_token;
            form.cloud_key = self.cloud_key;
            form.cloud_secret = self.cloud_secret;
        }

        form.save_token = self.save_token;
        form
    }
}
