use serde::{Deserialize, Serialize};
use docker_compose_types as dctypes;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Port {
    pub host_port: Option<String>,
    pub container_port: Option<String>,
}

impl TryInto<dctypes::Port> for &Port {
    type Error = String;
    fn try_into(self) -> Result<dctypes::Port, Self::Error> {
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

        Ok(dctypes::Port {
            target: cp,
            host_ip: None,
            published: Some(dctypes::PublishedPort::Single(hp)),
            protocol: None,
            mode: None,
        })
    }
}
