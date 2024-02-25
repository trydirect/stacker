use serde::{Deserialize, Serialize};
use docker_compose_types as dctypes;
use indexmap::IndexMap;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Volume {
    pub(crate) host_path: Option<String>,
    pub(crate) container_path: Option<String>,
}

impl Volume {
    pub fn is_named_docker_volume(&self) -> bool {
        // Docker named volumes typically don't contain special characters or slashes
        // They are alphanumeric and may include underscores or hyphens
        self
            .host_path
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
        // let's create a symlink to /var/docker/volumes in project docroot
        let mut driver_opts = IndexMap::default();
        let host_path = self.host_path.clone().unwrap_or_else(String::default);
        // @todo check if host_path is required argument
        driver_opts.insert(String::from("type"), Some(dctypes::SingleValue::String("none".to_string())));
        driver_opts.insert(String::from("o"), Some(dctypes::SingleValue::String("bind".to_string())));
        // @todo move to config stack docroot on host
        let path = format!("/root/stack/{}", &host_path);
        driver_opts.insert(String::from("device"), Some(dctypes::SingleValue::String(path)));

        dctypes::ComposeVolume {
            driver: Some(String::from("local")),
            driver_opts: driver_opts,
            external: None,
            labels: Default::default(),
            name: Some(host_path)
        }
    }
}

