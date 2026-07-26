//! Port / Exposure Audit (#5): flags services published to the public internet,
//! especially databases / admin UIs / sensitive default ports, and recommends
//! binding them to localhost or a private network. Static (compose-only).

use crate::compose::{parse_compose, ComposeService};
use crate::schema::{AuditReport, Cta, Finding, Severity};
use crate::score::build_report;

/// Well-known ports that should almost never be exposed publicly.
/// (port, human label, whether it is a datastore/admin surface)
const SENSITIVE_PORTS: &[(u32, &str)] = &[
    (5432, "PostgreSQL"),
    (3306, "MySQL/MariaDB"),
    (27017, "MongoDB"),
    (6379, "Redis"),
    (5672, "RabbitMQ"),
    (15672, "RabbitMQ management"),
    (9200, "Elasticsearch"),
    (11211, "Memcached"),
    (2375, "Docker daemon (unencrypted)"),
    (9000, "Portainer/MinIO admin"),
    (8080, "admin/app backend"),
];

fn sensitive_label(port: u32) -> Option<&'static str> {
    SENSITIVE_PORTS.iter().find(|(p, _)| *p == port).map(|(_, l)| *l)
}

fn audit_service(svc: &ComposeService) -> Vec<Finding> {
    let mut findings = Vec::new();
    for port in &svc.ports {
        if !port.is_public() {
            continue; // bound to loopback / a specific private IP — fine
        }
        // A published sensitive port is critical; any other public publish is info.
        if let Some(label) = sensitive_label(port.container_port) {
            findings.push(
                Finding::new(
                    "exposure.sensitive_port_public",
                    Severity::Critical,
                    format!("{label} port {} is exposed to the public internet", port.container_port),
                )
                .with_target(&svc.name)
                .with_detail(format!(
                    "Service '{}' publishes {} on all interfaces (0.0.0.0).",
                    svc.name, port.container_port
                ))
                .with_remediation(format!(
                    "Bind it to localhost — e.g. \"127.0.0.1:{p}:{p}\" — or keep it on an internal network with no `ports:` mapping.",
                    p = port.container_port
                )),
            );
        } else {
            findings.push(
                Finding::new(
                    "exposure.public_port",
                    Severity::Info,
                    format!("Port {} is published publicly", port.container_port),
                )
                .with_target(&svc.name)
                .with_remediation(
                    "Expose only the ports that genuinely need public access (usually just 80/443 via a reverse proxy).",
                ),
            );
        }
    }
    findings
}

/// Audit a raw compose file for public exposure issues.
pub fn audit_exposure(yaml: &str) -> AuditReport {
    let model = match parse_compose(yaml) {
        Ok(m) => m,
        Err(e) => {
            let f = Finding::new("exposure.unparsable", Severity::Critical, "Cannot parse compose")
                .with_detail(e.to_string());
            return build_report("exposure", vec![f], cta());
        }
    };
    let findings: Vec<Finding> = model.services.iter().flat_map(audit_service).collect();
    build_report("exposure", findings, cta())
}

fn cta() -> Option<Cta> {
    Some(Cta {
        label: "Lock this down and deploy on TryDirect →".to_string(),
        url: "https://try.direct/deploy".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Grade;

    #[test]
    fn public_database_port_is_critical() {
        let yaml = r#"
services:
  db:
    image: postgres:16
    ports: ["5432:5432"]
"#;
        let r = audit_exposure(yaml);
        assert!(r.findings.iter().any(|f| f.id == "exposure.sensitive_port_public"
            && f.severity == Severity::Critical
            && f.target.as_deref() == Some("db")));
        assert_eq!(r.grade, Grade::F);
    }

    #[test]
    fn loopback_bound_db_is_clean() {
        let yaml = r#"
services:
  db:
    image: postgres:16
    ports: ["127.0.0.1:5432:5432"]
"#;
        let r = audit_exposure(yaml);
        assert!(r.findings.is_empty(), "got: {:?}", r.findings);
        assert_eq!(r.grade, Grade::A);
    }

    #[test]
    fn public_web_port_is_only_info() {
        let yaml = r#"
services:
  web:
    image: nginx:1.27-alpine
    ports: ["80:80", "443:443"]
"#;
        let r = audit_exposure(yaml);
        assert!(r.findings.iter().all(|f| f.severity == Severity::Info));
        assert!(r.score >= 90);
    }

    #[test]
    fn internal_only_service_has_no_findings() {
        let yaml = r#"
services:
  worker:
    image: myorg/worker:1.0
"#;
        assert!(audit_exposure(yaml).findings.is_empty());
    }
}
