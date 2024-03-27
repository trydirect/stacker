use crate::models;
use serde::{Deserialize, Serialize, Serializer};
use serde_valid::Validate;
use crate::helpers::cloud::security::Secret;
use tracing::Instrument;


// fn hide_part<S>(value: &Option<String>, s: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
// {
//     eprintln!("value in serde {:?}", value);
//     let result: &str = match value {
//         Some(value) => {
//             let value = value.as_str();
//             value
//             // value.into_iter().take(6).collect::<String>()
//         }
//         None => "",
//     };
//     s.serialize_str(result)
// }
fn hide_parts(value: String) -> String {
    value.chars().into_iter().take(6).collect::<String>() + "****"
}


#[derive(Default, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Cloud {
    pub user_id: Option<String>,
    pub project_id: Option<i32>,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub provider: String,
    // #[serde(serialize_with = "hide_part")]
    pub cloud_token: Option<String>,
    // #[serde(serialize_with = "hide_part")]
    pub cloud_key: Option<String>,
    // #[serde(serialize_with = "hide_part")]
    pub cloud_secret: Option<String>,
    pub save_token: Option<bool>,
}

impl Cloud {
    pub(crate) fn decode(secret: &mut Secret, encrypted_value: String) -> String {
        // tracing::error!("encrypted_value {:?}", &encrypted_value);
        let b64_decoded = Secret::b64_decode(&encrypted_value).unwrap();
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
            let decoded_value = Cloud::decode(secret, val);
            if reveal {
                return Some(decoded_value);
            } else {
                return Some(hide_parts(decoded_value));
            }
        }
        None
    }

    // @todo should be refactored, may be moved to cloud.into() or Secret::from()
    pub(crate) fn decode_model(mut cloud: models::Cloud, reveal:bool) -> models::Cloud {

        let mut secret = Secret::new();
        secret.user_id = cloud.user_id.clone();
        secret.project_id = cloud.project_id.clone().unwrap();
        cloud.cloud_token = Cloud::decrypt_field(&mut secret, "cloud_token", cloud.cloud_token.clone(), reveal);
        cloud.cloud_secret = Cloud::decrypt_field(&mut secret, "cloud_secret", cloud.cloud_secret.clone(), reveal);
        cloud.cloud_key = Cloud::decrypt_field(&mut secret, "cloud_key", cloud.cloud_key.clone(), reveal);

        cloud
    }
}

impl std::fmt::Debug for Cloud {
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

impl Into<models::Cloud> for &Cloud {
    fn into(self) -> models::Cloud {
        let mut cloud = models::Cloud::default();
        cloud.provider = self.provider.clone();
        cloud.user_id = self.user_id.clone().unwrap();
        cloud.project_id = self.project_id;

        let mut secret = Secret::new();
        secret.user_id = self.user_id.clone().unwrap();
        secret.project_id = self.project_id.unwrap();

        cloud.cloud_token = encrypt_field(&mut secret, "cloud_token", self.cloud_token.clone());
        cloud.cloud_key = encrypt_field(&mut secret, "cloud_key", self.cloud_key.clone());
        cloud.cloud_secret = encrypt_field(&mut secret, "cloud_secret", self.cloud_secret.clone());
        cloud.save_token = self.save_token.clone();

        cloud
    }

}


// on deploy
impl Into<Cloud> for models::Cloud {
    fn into(self) -> Cloud {
        let mut form = Cloud::default();
        form.project_id = self.project_id;
        form.provider = self.provider;

        let mut secret = Secret::new();
        secret.user_id = self.user_id.clone();
        secret.project_id = self.project_id.clone().unwrap();

        secret.field = "cloud_token".to_string();
        let cloud_token = Cloud::decode(&mut secret, self.cloud_token.unwrap());
        form.cloud_token = Some(cloud_token);

        secret.field = "cloud_key".to_string();
        let cloud_key = Cloud::decode(&mut secret, self.cloud_key.unwrap());
        form.cloud_token = Some(cloud_key);

        secret.field = "cloud_secret".to_string();
        let cloud_secret = Cloud::decode(&mut secret, self.cloud_secret.unwrap());
        form.cloud_secret = Some(cloud_secret);

        form.save_token = self.save_token;

        form
    }
}
