use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::fmt;
use crate::helpers::dockerhub::DockerHub;


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerImage {
    // #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    // @todo conditional check, if not empty
    // #[validate(pattern = r"^[a-z0-9]+([-_.][a-z0-9]+)*$")]
    pub dockerhub_user: Option<String>,
    // #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    // @todo conditional check, if not empty
    // #[validate(pattern = r"^[a-z0-9]+([-_.][a-z0-9]+)*$")]
    pub dockerhub_name: Option<String>,
    // #[validate(min_length = 3)]
    #[validate(max_length = 100)]
    pub dockerhub_image: Option<String>,
    pub dockerhub_password: Option<String>,
}

impl fmt::Display for DockerImage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let dh_image = self.dockerhub_image.as_ref().map(String::as_str).unwrap_or("");
        let dh_nmspc = self.dockerhub_user.as_ref().map(String::as_str).unwrap_or("");
        let dh_name = self.dockerhub_name.as_ref().map(String::as_str).unwrap_or("");

        write!(
            f,
            "{}{}{}",
            if !dh_nmspc.is_empty() { format!("{}/", dh_nmspc) } else { String::new() },
            if !dh_name.is_empty() { dh_name } else { dh_image },
            if !dh_name.contains(":") && dh_image.is_empty() { ":latest".to_string() } else { String::new() },
        )
    }
}

impl DockerImage {
    #[tracing::instrument(name = "is_active")]
    pub async fn is_active(&self) -> Result<bool, String> {
        DockerHub::try_from(self)?.is_active().await
    }
}


