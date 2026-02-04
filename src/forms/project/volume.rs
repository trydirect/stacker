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
        let default_base =
            std::env::var("DEFAULT_DEPLOY_DIR").unwrap_or_else(|_| "/home/trydirect".to_string());
        let base = base_dir.unwrap_or(&default_base);

        let mut driver_opts = IndexMap::default();
        let host_path = self.host_path.clone().unwrap_or_else(String::default);

        driver_opts.insert(
            String::from("type"),
            Some(dctypes::SingleValue::String("none".to_string())),
        );
        driver_opts.insert(
            String::from("o"),
            Some(dctypes::SingleValue::String("bind".to_string())),
        );

        // Use configurable base directory instead of hardcoded /root/project
        let path = format!("{}/{}", base.trim_end_matches('/'), &host_path);
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
