//! A small typed docker-compose model shared by the exposure (#5), cost (#4)
//! and readiness (#6) checkers. Tolerant parser over `serde_yaml`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortMapping {
    /// Host bind IP, when the mapping pins one (e.g. "127.0.0.1").
    pub host_ip: Option<String>,
    /// Host (published) port. `None` = published to an ephemeral host port.
    pub host_port: Option<u32>,
    pub container_port: u32,
    pub protocol: String,
}

impl PortMapping {
    /// Published to every interface (0.0.0.0) — i.e. reachable from outside.
    pub fn is_public(&self) -> bool {
        match self.host_ip.as_deref() {
            None => true, // compose default bind is 0.0.0.0
            Some("0.0.0.0") | Some("::") => true,
            Some(_) => false, // e.g. 127.0.0.1 -> loopback only
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ComposeService {
    pub name: String,
    pub image: Option<String>,
    pub ports: Vec<PortMapping>,
    /// CPU limit (or reservation) in fractional cores, if declared.
    pub cpus: Option<f64>,
    /// Memory limit (or reservation) in MB, if declared.
    pub memory_mb: Option<u64>,
    pub restart: Option<String>,
    pub has_healthcheck: bool,
    pub privileged: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ComposeModel {
    pub services: Vec<ComposeService>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid YAML: {0}")]
    Yaml(String),
    #[error("no `services` mapping found")]
    NoServices,
}

/// Parse memory strings like "128M", "1g", "512m", "1073741824" (bytes) into MB.
pub fn parse_memory_mb(v: &serde_json::Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return Some(n / (1024 * 1024)); // raw bytes
    }
    let s = v.as_str()?.trim().to_lowercase();
    let (num, mult): (&str, u64) =
        if let Some(p) = s.strip_suffix("gb").or_else(|| s.strip_suffix('g')) {
            (p, 1024)
        } else if let Some(p) = s.strip_suffix("mb").or_else(|| s.strip_suffix('m')) {
            (p, 1)
        } else if let Some(p) = s.strip_suffix("kb").or_else(|| s.strip_suffix('k')) {
            return p.trim().parse::<u64>().ok().map(|k| k / 1024);
        } else if let Some(p) = s.strip_suffix('b') {
            return p.trim().parse::<u64>().ok().map(|b| b / (1024 * 1024));
        } else {
            (s.as_str(), 1)
        };
    num.trim()
        .parse::<f64>()
        .ok()
        .map(|n| (n * mult as f64) as u64)
}

fn parse_cpus(v: &serde_json::Value) -> Option<f64> {
    if let Some(f) = v.as_f64() {
        return Some(f);
    }
    v.as_str().and_then(|s| s.trim().parse::<f64>().ok())
}

/// Parse a single compose port entry (short "h:c", "ip:h:c", "c", or long form).
fn parse_port_entry(entry: &serde_json::Value) -> Option<PortMapping> {
    // Long form: { target, published, host_ip, protocol }
    if let Some(obj) = entry.as_object() {
        let container_port = obj.get("target").and_then(port_num)?;
        return Some(PortMapping {
            host_ip: obj
                .get("host_ip")
                .and_then(|v| v.as_str())
                .map(String::from),
            host_port: obj.get("published").and_then(port_num),
            container_port,
            protocol: obj
                .get("protocol")
                .and_then(|v| v.as_str())
                .unwrap_or("tcp")
                .to_string(),
        });
    }
    // Short form string / number.
    let raw = match entry {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let (spec, protocol) = match raw.split_once('/') {
        Some((s, p)) => (s.to_string(), p.to_string()),
        None => (raw, "tcp".to_string()),
    };
    let parts: Vec<&str> = spec.split(':').collect();
    let (host_ip, host_port, container_port) = match parts.as_slice() {
        [c] => (None, None, c.parse().ok()?),
        [h, c] => (None, h.parse().ok(), c.parse().ok()?),
        [ip, h, c] => (Some(ip.to_string()), h.parse().ok(), c.parse().ok()?),
        _ => return None,
    };
    Some(PortMapping {
        host_ip,
        host_port,
        container_port,
        protocol,
    })
}

fn port_num(v: &serde_json::Value) -> Option<u32> {
    v.as_u64()
        .map(|n| n as u32)
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Parse a raw compose YAML string into the typed model.
pub fn parse_compose(yaml: &str) -> Result<ComposeModel, ParseError> {
    let value: serde_json::Value =
        serde_yaml::from_str(yaml).map_err(|e| ParseError::Yaml(e.to_string()))?;
    let services_map = value
        .get("services")
        .and_then(|v| v.as_object())
        .ok_or(ParseError::NoServices)?;

    let mut services = Vec::new();
    for (name, svc) in services_map {
        let ports = svc
            .get("ports")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(parse_port_entry).collect())
            .unwrap_or_default();

        // Resource limits: prefer limits, fall back to reservations.
        let resources = svc.get("deploy").and_then(|d| d.get("resources"));
        let pick = |key: &str, field: &str| -> Option<serde_json::Value> {
            resources
                .and_then(|r| r.get(key))
                .and_then(|l| l.get(field))
                .cloned()
        };
        let cpus = pick("limits", "cpus")
            .or_else(|| pick("reservations", "cpus"))
            .as_ref()
            .and_then(parse_cpus);
        let memory_mb = pick("limits", "memory")
            .or_else(|| pick("reservations", "memory"))
            .as_ref()
            .and_then(parse_memory_mb);

        services.push(ComposeService {
            name: name.clone(),
            image: svc.get("image").and_then(|v| v.as_str()).map(String::from),
            ports,
            cpus,
            memory_mb,
            restart: svc
                .get("restart")
                .and_then(|v| v.as_str())
                .map(String::from),
            has_healthcheck: svc.get("healthcheck").is_some(),
            privileged: svc
                .get("privileged")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        });
    }
    services.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ComposeModel { services })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_memory_units() {
        assert_eq!(parse_memory_mb(&json!("128M")), Some(128));
        assert_eq!(parse_memory_mb(&json!("1g")), Some(1024));
        assert_eq!(parse_memory_mb(&json!("512m")), Some(512));
        assert_eq!(parse_memory_mb(&json!(134217728u64)), Some(128)); // raw bytes
    }

    #[test]
    fn parses_port_forms() {
        // host:container
        let p = parse_port_entry(&json!("8080:80")).unwrap();
        assert_eq!((p.host_port, p.container_port), (Some(8080), 80));
        assert!(p.is_public());
        // ip:host:container -> loopback, not public
        let p = parse_port_entry(&json!("127.0.0.1:5432:5432")).unwrap();
        assert_eq!(p.host_ip.as_deref(), Some("127.0.0.1"));
        assert!(!p.is_public());
        // container only
        let p = parse_port_entry(&json!("80")).unwrap();
        assert_eq!(p.container_port, 80);
        // long form
        let p =
            parse_port_entry(&json!({"target":80,"published":8080,"host_ip":"0.0.0.0"})).unwrap();
        assert_eq!((p.host_port, p.container_port), (Some(8080), 80));
        assert!(p.is_public());
    }

    #[test]
    fn parses_services_with_resources() {
        let yaml = r#"
services:
  db:
    image: postgres:16
    ports: ["127.0.0.1:5432:5432"]
    restart: unless-stopped
    deploy:
      resources:
        limits: { cpus: "0.5", memory: 256M }
    healthcheck:
      test: ["CMD", "pg_isready"]
"#;
        let m = parse_compose(yaml).unwrap();
        assert_eq!(m.services.len(), 1);
        let db = &m.services[0];
        assert_eq!(db.name, "db");
        assert_eq!(db.image.as_deref(), Some("postgres:16"));
        assert_eq!(db.cpus, Some(0.5));
        assert_eq!(db.memory_mb, Some(256));
        assert!(db.has_healthcheck);
        assert_eq!(db.restart.as_deref(), Some("unless-stopped"));
        assert!(!db.ports[0].is_public());
    }

    #[test]
    fn missing_services_is_error() {
        assert!(matches!(
            parse_compose("version: '3'").unwrap_err(),
            ParseError::NoServices
        ));
    }
}
