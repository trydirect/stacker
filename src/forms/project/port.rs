use docker_compose_types as dctypes;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Port {
    #[validate(custom(|v| validate_non_empty(v)))]
    pub host_port: Option<String>,
    #[validate(custom(|v| validate_container_port(v)))]
    pub container_port: String,
    #[validate(enumerate("tcp", "udp"))]
    pub protocol: Option<String>,
}

/// Parse a port and check it is actually in range.
///
/// The previous rules only counted digits, so 133342 validated and reached the
/// target host, where docker compose rejected it with "invalid containerPort".
/// The two-character minimum is pre-existing behaviour and is kept deliberately.
fn parse_port_in_range(value: &str) -> Result<u16, String> {
    if value.len() < 2 {
        return Err(format!("Port is not valid: {value}"));
    }

    // Parse wide, then range-check, so an out-of-range value reports the range
    // rather than a u16 overflow.
    let parsed: u64 = value
        .parse()
        .map_err(|_| format!("Port is not valid: {value}"))?;

    if parsed == 0 || parsed > u16::MAX as u64 {
        return Err(format!(
            "Port {value} is out of range, must be between 1 and {}",
            u16::MAX
        ));
    }

    Ok(parsed as u16)
}

/// container_port may carry a whole mapping ("1025:25", "127.0.0.1:8080:80",
/// "80/tcp"), so range-check every numeric segment rather than the raw string.
fn validate_container_port(v: &String) -> Result<(), serde_valid::validation::Error> {
    let without_proto = v.split('/').next().unwrap_or(v.as_str());

    for segment in without_proto.split(':') {
        // Skip a host IP; only the numeric segments are ports.
        if segment.contains('.') {
            continue;
        }
        if let Err(err) = parse_port_in_range(segment) {
            return Err(serde_valid::validation::Error::Custom(err));
        }
    }

    Ok(())
}

fn validate_non_empty(v: &Option<String>) -> Result<(), serde_valid::validation::Error> {
    if v.is_none() {
        return Ok(());
    }

    if let Some(value) = v {
        if value.is_empty() {
            return Ok(());
        }

        if let Err(err) = parse_port_in_range(value.as_str()) {
            return Err(serde_valid::validation::Error::Custom(
                err,
            ));
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
        let normalized = normalize_port_mapping(self);

        let cp = parse_port_in_range(normalized.container_port.as_str())
            .map_err(|err| format!("Could not parse container port: {err}"))?;

        let hp = match normalized.host_port {
            Some(hp) => {
                if hp.is_empty() {
                    None
                } else {
                    // Previously a bad host port was logged at debug and
                    // dropped to None, publishing the mapping without it.
                    match parse_port_in_range(hp.as_str()) {
                        Ok(port) => Some(dctypes::PublishedPort::Single(port)),
                        Err(err) => return Err(err),
                    }
                }
            }
            _ => None,
        };

        tracing::debug!("Port conversion result: cp: {:?} hp: {:?}", cp, hp);

        Ok(dctypes::Port {
            target: cp,
            host_ip: normalized.host_ip,
            published: hp,
            protocol: self.protocol.clone(),
            mode: None,
        })
    }
}

struct NormalizedPortMapping {
    host_ip: Option<String>,
    host_port: Option<String>,
    container_port: String,
}

fn normalize_port_mapping(port: &Port) -> NormalizedPortMapping {
    let container_no_proto = port
        .container_port
        .split('/')
        .next()
        .unwrap_or(port.container_port.as_str());

    if let Some((host_part, container_port)) = container_no_proto.rsplit_once(':') {
        let (host_ip, host_port) = match host_part.rsplit_once(':') {
            Some((ip, published)) => (Some(ip.to_string()), Some(published.to_string())),
            None => match port.host_port.as_deref() {
                Some(host) if host.parse::<u16>().is_err() => {
                    (Some(host.to_string()), Some(host_part.to_string()))
                }
                _ => (None, Some(host_part.to_string())),
            },
        };

        return NormalizedPortMapping {
            host_ip,
            host_port,
            container_port: container_port.to_string(),
        };
    }

    NormalizedPortMapping {
        host_ip: None,
        host_port: port.host_port.clone(),
        container_port: container_no_proto.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Regression: ports must be a real port number, not just N digits ---
    //
    // A GitLab deploy failed on the target host with
    //   docker compose ... stderr: "invalid containerPort: 133342"
    // after the value passed validation here: the rules checked digit COUNT
    // (2-6 digits), not the 1..=65535 range, so any 6-digit number was accepted.

    #[test]
    fn host_port_rejects_out_of_range() {
        assert!(validate_non_empty(&Some("133342".to_string())).is_err());
        assert!(validate_non_empty(&Some("65536".to_string())).is_err());
        assert!(validate_non_empty(&Some("99999".to_string())).is_err());
    }

    #[test]
    fn host_port_accepts_the_upper_boundary() {
        assert!(validate_non_empty(&Some("65535".to_string())).is_ok());
    }

    #[test]
    fn container_port_rejects_out_of_range() {
        for bad in ["133342", "65536", "99999"] {
            let port = Port {
                host_port: None,
                container_port: bad.to_string(),
                protocol: Some("tcp".to_string()),
            };
            assert!(
                port.validate().is_err(),
                "container_port {bad} should be rejected"
            );
        }
    }

    #[test]
    fn container_port_rejects_more_than_five_digits() {
        // Guards the `{2,6}+` quantifier: whatever it means to the regex
        // engine, a 7-digit value must not validate.
        let port = Port {
            host_port: None,
            container_port: "1234567".to_string(),
            protocol: Some("tcp".to_string()),
        };
        assert!(port.validate().is_err());
    }

    #[test]
    fn container_port_accepts_the_upper_boundary() {
        let port = Port {
            host_port: None,
            container_port: "65535".to_string(),
            protocol: Some("tcp".to_string()),
        };
        assert!(port.validate().is_ok());
    }

    #[test]
    fn out_of_range_host_port_is_an_error_not_a_silent_drop() {
        // Previously an unparseable host port was logged at debug and turned
        // into None, so the mapping was published without it and nobody knew.
        let port = Port {
            host_port: None,
            container_port: "133342:80".to_string(),
            protocol: Some("tcp".to_string()),
        };
        let result: Result<dctypes::Port, String> = (&port).try_into();
        assert!(result.is_err(), "expected an error, got {result:?}");
    }

    #[test]
    fn test_validate_non_empty_none() {
        assert!(validate_non_empty(&None).is_ok());
    }

    #[test]
    fn test_validate_non_empty_empty_string() {
        assert!(validate_non_empty(&Some("".to_string())).is_ok());
    }

    #[test]
    fn test_validate_non_empty_valid_port() {
        assert!(validate_non_empty(&Some("8080".to_string())).is_ok());
        assert!(validate_non_empty(&Some("80".to_string())).is_ok());
        assert!(validate_non_empty(&Some("443".to_string())).is_ok());
    }

    #[test]
    fn test_validate_non_empty_invalid_port() {
        assert!(validate_non_empty(&Some("abc".to_string())).is_err());
        assert!(validate_non_empty(&Some("1".to_string())).is_err()); // too short (min 2 digits)
        assert!(validate_non_empty(&Some("1234567".to_string())).is_err()); // too long (max 6 digits)
    }

    #[test]
    fn test_port_try_into_valid() {
        let port = Port {
            host_port: Some("8080".to_string()),
            container_port: "80".to_string(),
            protocol: Some("tcp".to_string()),
        };
        let result: Result<dctypes::Port, String> = (&port).try_into();
        assert!(result.is_ok());
        let dc_port = result.unwrap();
        assert_eq!(dc_port.target, 80);
    }

    #[test]
    fn test_port_try_into_accepts_host_ip_mapping_in_container_port() {
        let port = Port {
            host_port: Some("127.0.0.1".to_string()),
            container_port: "1025:25".to_string(),
            protocol: Some("tcp".to_string()),
        };

        let result: Result<dctypes::Port, String> = (&port).try_into();

        assert!(result.is_ok());
        let dc_port = result.unwrap();
        assert_eq!(dc_port.target, 25);
        assert_eq!(dc_port.host_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(
            dc_port.published,
            Some(dctypes::PublishedPort::Single(1025))
        );
        assert_eq!(dc_port.protocol.as_deref(), Some("tcp"));
    }

    #[test]
    fn test_port_try_into_accepts_full_compose_mapping_in_container_port() {
        let port = Port {
            host_port: None,
            container_port: "127.0.0.1:1025:25/tcp".to_string(),
            protocol: None,
        };

        let result: Result<dctypes::Port, String> = (&port).try_into();

        assert!(result.is_ok());
        let dc_port = result.unwrap();
        assert_eq!(dc_port.target, 25);
        assert_eq!(dc_port.host_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(
            dc_port.published,
            Some(dctypes::PublishedPort::Single(1025))
        );
    }

    #[test]
    fn test_port_try_into_no_host_port() {
        let port = Port {
            host_port: None,
            container_port: "3000".to_string(),
            protocol: None,
        };
        let result: Result<dctypes::Port, String> = (&port).try_into();
        assert!(result.is_ok());
        let dc_port = result.unwrap();
        assert_eq!(dc_port.target, 3000);
        assert!(dc_port.published.is_none());
    }

    #[test]
    fn test_port_try_into_empty_host_port() {
        let port = Port {
            host_port: Some("".to_string()),
            container_port: "5432".to_string(),
            protocol: None,
        };
        let result: Result<dctypes::Port, String> = (&port).try_into();
        assert!(result.is_ok());
        let dc_port = result.unwrap();
        assert!(dc_port.published.is_none());
    }

    #[test]
    fn test_port_try_into_invalid_container_port() {
        let port = Port {
            host_port: None,
            container_port: "not_a_number".to_string(),
            protocol: None,
        };
        let result: Result<dctypes::Port, String> = (&port).try_into();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Could not parse container port"));
    }

    #[test]
    fn test_port_default() {
        let port = Port::default();
        assert!(port.host_port.is_none());
        assert_eq!(port.container_port, "");
        assert!(port.protocol.is_none());
    }

    #[test]
    fn test_port_serialization() {
        let port = Port {
            host_port: Some("8080".to_string()),
            container_port: "80".to_string(),
            protocol: Some("tcp".to_string()),
        };
        let json = serde_json::to_string(&port).unwrap();
        let deserialized: Port = serde_json::from_str(&json).unwrap();
        assert_eq!(port, deserialized);
    }
}
