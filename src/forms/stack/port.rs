use serde::{Deserialize, Serialize};
use docker_compose_types as dctypes;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Port {
    pub host_port: Option<String>,
    pub container_port: Option<String>,
}

// impl Default for Port{
//     fn default() -> Self {
//         Port {
//             target: 80,
//             host_ip: None,
//             published: None,
//             protocol: None,
//             mode: None,
//         }
//     }
// }

impl TryInto<dctypes::Port> for &Port {
    type Error = String;
    fn try_into(self) -> Result<dctypes::Port, Self::Error> {
        let cp = self
            .container_port
            .as_ref()
            .map_or(Ok(0u16), |s| s.parse::<u16>())
            .map_err(|_| "Could not parse container port".to_string())?;

        let hp = match self.host_port.clone() {
            Some(hp) => {
                if hp.is_empty() {
                    None
                } else {
                    match hp.parse::<u16>() {
                        Ok(port) => Some(dctypes::PublishedPort::Single(port)),
                        Err(_) => {
                            tracing::debug!("Could not parse host port: {}", hp);
                            None
                        }
                    }
                }
            }
            _ => None
        };

        tracing::debug!("Port conversion result: cp: {:?} hp: {:?}", cp, hp);

        Ok(dctypes::Port {
            target: cp,
            host_ip: None,
            published: hp,
            protocol: None,
            mode: None,
        })
    }
}
