use crate::helpers::dockerhub::DockerHub;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::fmt;

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
    /// Tag portion of the image (e.g. `15-alpine` for `postgres:15-alpine`).
    ///
    /// Older callers used to embed the tag directly inside `dockerhub_name`
    /// (`postgres:15-alpine`). The CLI's `parse_docker_image` splits the tag
    /// into a separate `dockerhub_tag` field, so this field is required to
    /// preserve version pinning across the wire — without it the server
    /// rebuilds the image string from `dockerhub_name` alone and falls back
    /// to `:latest`, silently unpinning services like `postgres:15-alpine`
    /// to `postgres:latest`.
    #[validate(max_length = 128)]
    pub dockerhub_tag: Option<String>,
}

impl fmt::Display for DockerImage {
    // dh_image = trydirect/postgres:latest
    // dh_nmsp = trydirect, dh_repo_name=postgres
    // dh_nmsp = trydirect dh_repo_name=postgres:v8
    // namespace/repo_name/tag
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let dh_image = self.dockerhub_image.as_deref().unwrap_or("");
        let dh_nmspc = self.dockerhub_user.as_deref().unwrap_or("");
        let dh_repo_name = self.dockerhub_name.as_deref().unwrap_or("");
        let dh_tag = self.dockerhub_tag.as_deref().unwrap_or("");

        let nmspc = if !dh_nmspc.is_empty() {
            format!("{}/", dh_nmspc)
        } else {
            String::new()
        };

        let body = if !dh_repo_name.is_empty() {
            dh_repo_name
        } else {
            dh_image
        };

        // Don't double-add a tag if the body (name or image) already carries
        // one — covers callers that still inline the tag in
        // `dockerhub_name` (e.g. `postgres:v8`) or `dockerhub_image`
        // (e.g. `nginx:latest`).
        let suffix = if body.contains(':') || body.contains('@') {
            String::new()
        } else if !dh_tag.is_empty() {
            format!(":{}", dh_tag)
        } else if dh_image.is_empty() {
            // We only fell into the `:latest` historical default when the
            // display was synthesized from `dockerhub_name` alone (no image
            // fallback). Keep that behavior for callers that omit the tag
            // so we don't change the contract for already-valid `image-only`
            // entries.
            ":latest".to_string()
        } else {
            String::new()
        };

        write!(f, "{nmspc}{body}{suffix}")
    }
}

impl DockerImage {
    #[tracing::instrument(name = "is_active", skip_all)]
    pub async fn is_active(&self) -> Result<bool, String> {
        DockerHub::try_from(self)?.is_active().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_full_image() {
        let img = DockerImage {
            dockerhub_user: Some("trydirect".to_string()),
            dockerhub_name: Some("postgres:v8".to_string()),
            dockerhub_image: None,
            dockerhub_password: None,
            dockerhub_tag: None,
        };
        assert_eq!(format!("{}", img), "trydirect/postgres:v8");
    }

    #[test]
    fn test_display_image_only() {
        let img = DockerImage {
            dockerhub_user: None,
            dockerhub_name: None,
            dockerhub_image: Some("nginx:latest".to_string()),
            dockerhub_password: None,
            dockerhub_tag: None,
        };
        assert_eq!(format!("{}", img), "nginx:latest");
    }

    #[test]
    fn test_display_name_without_tag_adds_latest() {
        let img = DockerImage {
            dockerhub_user: Some("myuser".to_string()),
            dockerhub_name: Some("myapp".to_string()),
            dockerhub_image: None,
            dockerhub_password: None,
            dockerhub_tag: None,
        };
        assert_eq!(format!("{}", img), "myuser/myapp:latest");
    }

    #[test]
    fn test_display_name_with_tag_no_latest() {
        let img = DockerImage {
            dockerhub_user: Some("myuser".to_string()),
            dockerhub_name: Some("myapp:v2".to_string()),
            dockerhub_image: None,
            dockerhub_password: None,
            dockerhub_tag: None,
        };
        assert_eq!(format!("{}", img), "myuser/myapp:v2");
    }

    #[test]
    fn test_display_no_user_with_name() {
        let img = DockerImage {
            dockerhub_user: None,
            dockerhub_name: Some("redis".to_string()),
            dockerhub_image: None,
            dockerhub_password: None,
            dockerhub_tag: None,
        };
        assert_eq!(format!("{}", img), "redis:latest");
    }

    #[test]
    fn test_display_all_empty() {
        let img = DockerImage::default();
        assert_eq!(format!("{}", img), ":latest");
    }

    #[test]
    fn test_display_image_takes_precedence_when_name_empty() {
        let img = DockerImage {
            dockerhub_user: None,
            dockerhub_name: None,
            dockerhub_image: Some("custom/image:tag".to_string()),
            dockerhub_password: None,
            dockerhub_tag: None,
        };
        assert_eq!(format!("{}", img), "custom/image:tag");
    }

    #[test]
    fn test_docker_image_serialization() {
        let img = DockerImage {
            dockerhub_user: Some("user".to_string()),
            dockerhub_name: Some("app".to_string()),
            dockerhub_image: Some("user/app:1.0".to_string()),
            dockerhub_password: None,
            dockerhub_tag: None,
        };
        let json = serde_json::to_string(&img).unwrap();
        let deserialized: DockerImage = serde_json::from_str(&json).unwrap();
        assert_eq!(img, deserialized);
    }

    #[test]
    fn test_display_name_with_detached_tag_preserves_version_pin() {
        // Mirrors the JSON the CLI emits for `image: postgres:15-alpine`:
        // dockerhub_name="postgres", dockerhub_tag="15-alpine". Before the
        // dockerhub_tag field existed, the tag was dropped server-side and
        // Display rebuilt `postgres:latest`.
        let img = DockerImage {
            dockerhub_user: None,
            dockerhub_name: Some("postgres".to_string()),
            dockerhub_image: None,
            dockerhub_password: None,
            dockerhub_tag: Some("15-alpine".to_string()),
        };
        assert_eq!(format!("{}", img), "postgres:15-alpine");
    }

    #[test]
    fn test_display_namespaced_image_with_detached_tag() {
        let img = DockerImage {
            dockerhub_user: Some("trydirect".to_string()),
            dockerhub_name: Some("postgres".to_string()),
            dockerhub_image: None,
            dockerhub_password: None,
            dockerhub_tag: Some("15-alpine".to_string()),
        };
        assert_eq!(format!("{}", img), "trydirect/postgres:15-alpine");
    }

    #[test]
    fn test_display_inline_tag_in_name_takes_precedence_over_detached_tag() {
        // If a caller inlines the tag in `dockerhub_name`, we must not
        // append `dockerhub_tag` on top of it (would yield `app:v2:v2`).
        let img = DockerImage {
            dockerhub_user: Some("myuser".to_string()),
            dockerhub_name: Some("myapp:v2".to_string()),
            dockerhub_image: None,
            dockerhub_password: None,
            dockerhub_tag: Some("v2".to_string()),
        };
        assert_eq!(format!("{}", img), "myuser/myapp:v2");
    }

    #[test]
    fn test_display_detached_tag_applied_after_image_only_body_without_tag() {
        let img = DockerImage {
            dockerhub_user: None,
            dockerhub_name: None,
            dockerhub_image: Some("nginx".to_string()),
            dockerhub_password: None,
            dockerhub_tag: Some("1.27".to_string()),
        };
        assert_eq!(format!("{}", img), "nginx:1.27");
    }

    #[test]
    fn test_deserialize_captures_dockerhub_tag_field() {
        // Confirms the JSON object the CLI sends (`dockerhub_tag` next to
        // `dockerhub_name` / `dockerhub_user`) round-trips into the struct,
        // which is what the App `#[serde(flatten)]` relies on server-side.
        let json = r#"{
            "dockerhub_user": null,
            "dockerhub_name": "postgres",
            "dockerhub_image": null,
            "dockerhub_password": null,
            "dockerhub_tag": "15-alpine"
        }"#;
        let img: DockerImage = serde_json::from_str(json).unwrap();
        assert_eq!(img.dockerhub_tag.as_deref(), Some("15-alpine"));
        assert_eq!(format!("{}", img), "postgres:15-alpine");
    }
}
