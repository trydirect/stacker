use serde::{Deserialize, Serialize};
use docker_compose_types as dctypes;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Volume {
    pub(crate) host_path: Option<String>,
    pub(crate) container_path: Option<String>,
}

impl Volume {
    pub fn is_named_docker(&self) -> bool {
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
        Ok(dctypes::AdvancedVolumes {
            source: source,
            target: target.unwrap_or("".to_string()),
            _type: "".to_string(),
            read_only: false,
            bind: None,
            volume: None,
            tmpfs: None,
        })
    }
}
