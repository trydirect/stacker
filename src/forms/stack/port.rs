use serde::{Deserialize, Serialize};
use docker_compose_types as dctypes;
use regex::Regex;
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Port {
    #[validate(custom(|v| validate_non_empty(v)))]
    pub host_port: Option<String>,
    #[validate(pattern = r"^\d{2,6}+$")]
    pub container_port: String,
    #[validate(enumerate("tcp", "udp"))]
    pub protocol: Option<String>,
}

fn validate_non_empty(v: &Option<String>) -> Result<(), serde_valid::validation::Error> {
    if v.is_none() {
        return Ok(());
    }

    if let Some(value) = v {
        if value.is_empty() {
            return Ok(());
        }

        let re = Regex::new(r"^\d{2,6}$").unwrap();

        if !re.is_match(value.as_str()) {
            return Err(serde_valid::validation::Error::Custom("Port is not valid.".to_owned()));
        }
    }

    Ok(())
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
