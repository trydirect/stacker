use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::fmt;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerImage {
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub dockerhub_user: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub dockerhub_name: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 100)]
    pub dockerhub_image: Option<String>,
}

impl fmt::Display for DockerImage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let tag = "latest";

        let dim = self.dockerhub_image.clone().unwrap_or("".to_string());
        write!(
            f,
            "{}/{}:{}",
            self.dockerhub_user
                .clone()
                .unwrap_or("trydirect".to_string())
                .clone(),
            self.dockerhub_name.clone().unwrap_or(dim),
            tag
        )
    }
}
