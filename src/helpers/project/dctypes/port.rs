use serde::{Deserialize, Serialize};
use crate::helpers::project::dctypes;
use crate::forms;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Port {
    pub target: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<dctypes::PublishedPort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl Default for Port {
    fn default() -> Self {
        Port {
            target: 80,
            host_ip: None,
            published: None,
            protocol: None,
            mode: None,
        }
    }
}

impl TryInto<Port> for &forms::project::Port {
    type Error = String;
    fn try_into(self) -> Result<Port, Self::Error> {
        let cp = self
            .container_port
            .as_ref()
            .map_or(Ok(0u16), |s| s.parse::<u16>())
            .map_err(|_| "Could not parse port".to_string())?;

        let hp = self
            .host_port
            .as_ref()
            .map_or(Ok(0u16), |s| s.parse::<u16>())
            .map_err(|_| "Could not parse port".to_string())?;

        Ok(Port {
            target: cp,
            host_ip: None,
            published: Some(dctypes::PublishedPort::Single(hp)),
            protocol: None,
            mode: None,
        })
    }
}
