use docker_compose_types as dctypes;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Volume {
    pub host_path: Option<String>,
    pub container_path: Option<String>,
}

impl Volume {
    pub fn is_named_docker_volume(&self) -> bool {
        // Docker named volumes typically don't contain special characters or slashes
        // They are alphanumeric and may include underscores or hyphens
        self.host_path
            .as_ref()
            .unwrap()
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    }
}

impl TryInto<dctypes::AdvancedVolumes> for &Volume {
    type Error = String;
    fn try_into(self) -> Result<dctypes::AdvancedVolumes, Self::Error> {
        let source = self.host_path.clone();
        let target = self.container_path.clone();
        tracing::debug!(
            "Volume conversion result: source: {:?} target: {:?}",
            source,
            target
        );

        let _type = if self.is_named_docker_volume() {
            "volume"
        } else {
            "bind"
        };

        Ok(dctypes::AdvancedVolumes {
            source: source,
            target: target.unwrap_or("".to_string()),
            _type: _type.to_string(),
            read_only: false,
            bind: None,
            volume: None,
            tmpfs: None,
        })
    }
}

impl Into<dctypes::ComposeVolume> for &Volume {
    fn into(self) -> dctypes::ComposeVolume {
        // Use default base dir - for custom base dir use to_compose_volume()
        self.to_compose_volume(None)
    }
}

impl Volume {
    /// Convert to ComposeVolume with optional custom base directory
    /// If base_dir is None, uses DEFAULT_DEPLOY_DIR env var or "/home/trydirect"
    pub fn to_compose_volume(&self, base_dir: Option<&str>) -> dctypes::ComposeVolume {
        let host_path = self.host_path.clone().unwrap_or_else(String::default);

        if self.is_named_docker_volume() {
            return dctypes::ComposeVolume {
                driver: None,
                driver_opts: Default::default(),
                external: None,
                labels: Default::default(),
                name: Some(host_path),
            };
        }

        let default_base =
            std::env::var("DEFAULT_DEPLOY_DIR").unwrap_or_else(|_| "/home/trydirect".to_string());
        let base = base_dir.unwrap_or(&default_base);

        let mut driver_opts = IndexMap::default();

        driver_opts.insert(
            String::from("type"),
            Some(dctypes::SingleValue::String("none".to_string())),
        );
        driver_opts.insert(
            String::from("o"),
            Some(dctypes::SingleValue::String("bind".to_string())),
        );

        // Normalize to avoid duplicate slashes in bind-mount device paths.
        let normalized_host = host_path
            .trim_start_matches("./")
            .trim_start_matches('/');
        let path = format!("{}/{}", base.trim_end_matches('/'), normalized_host);
        driver_opts.insert(
            String::from("device"),
            Some(dctypes::SingleValue::String(path)),
        );

        dctypes::ComposeVolume {
            driver: Some(String::from("local")),
            driver_opts: driver_opts,
            external: None,
            labels: Default::default(),
            name: Some(host_path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Volume;
    use docker_compose_types::SingleValue;

    #[test]
    fn test_named_volume_is_not_prefixed() {
        let volume = Volume {
            host_path: Some("redis_data".to_string()),
            container_path: Some("/data".to_string()),
        };

        let compose = volume.to_compose_volume(Some("/custom/base"));

        assert!(compose.driver.is_none());
        assert!(compose.driver_opts.is_empty());
        assert_eq!(compose.name.as_deref(), Some("redis_data"));
    }

    #[test]
    fn test_bind_volume_is_prefixed_with_base_dir() {
        let volume = Volume {
            host_path: Some("projects/app".to_string()),
            container_path: Some("/var/lib/app".to_string()),
        };

        let compose = volume.to_compose_volume(Some("/srv/trydirect"));
        let device = compose
            .driver_opts
            .get("device")
            .and_then(|v| v.as_ref());

        assert_eq!(compose.driver.as_deref(), Some("local"));
        assert_eq!(compose.name.as_deref(), Some("projects/app"));
        assert_eq!(device, Some(&SingleValue::String("/srv/trydirect/projects/app".to_string())));
    }

    #[test]
    fn test_bind_volume_absolute_path() {
        let volume = Volume {
            host_path: Some("/data".to_string()),
            container_path: Some("/var/lib/data".to_string()),
        };

        let compose = volume.to_compose_volume(Some("/srv/trydirect"));
        let device = compose
            .driver_opts
            .get("device")
            .and_then(|v| v.as_ref());

        assert!(!volume.is_named_docker_volume());
        assert_eq!(compose.driver.as_deref(), Some("local"));
        assert_eq!(device, Some(&SingleValue::String("/srv/trydirect/data".to_string())));
    }

    #[test]
    fn test_bind_volume_relative_path() {
        let volume = Volume {
            host_path: Some("./data".to_string()),
            container_path: Some("/var/lib/data".to_string()),
        };

        let compose = volume.to_compose_volume(Some("/srv/trydirect"));
        let device = compose
            .driver_opts
            .get("device")
            .and_then(|v| v.as_ref());

        assert!(!volume.is_named_docker_volume());
        assert_eq!(compose.driver.as_deref(), Some("local"));
        assert_eq!(device, Some(&SingleValue::String("/srv/trydirect/data".to_string())));
    }

    #[test]
    fn test_is_named_docker_volume() {
        let named = Volume {
            host_path: Some("data_store-1".to_string()),
            container_path: None,
        };
        let bind = Volume {
            host_path: Some("/var/lib/app".to_string()),
            container_path: None,
        };

        assert!(named.is_named_docker_volume());
        assert!(!bind.is_named_docker_volume());
    }
}
