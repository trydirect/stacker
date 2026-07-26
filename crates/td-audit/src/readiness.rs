//! Stack Production-Readiness Score (#6): a composite grade over the compose
//! security audit (#1), exposure audit (#5), and operational checks (restart
//! policy, healthchecks, resource limits) derived from the compose model.

use crate::compose::{audit_compose, parse_compose};
use crate::exposure::audit_exposure;
use crate::schema::{AuditReport, Cta, Finding, Severity};
use crate::score::build_report;

/// Audit a compose file for overall production readiness.
pub fn audit_readiness(yaml: &str) -> AuditReport {
    let model = match parse_compose(yaml) {
        Err(e) => {
            let f = Finding::new("readiness.unparsable", Severity::Critical, "Cannot parse compose")
                .with_detail(e.to_string());
            return build_report("readiness", vec![f], cta());
        }
        Ok(m) => m,
    };

    // Roll in the security + exposure findings (their own ids are preserved).
    let mut findings = audit_compose(yaml).findings;
    findings.extend(audit_exposure(yaml).findings);

    // Operational readiness, per service.
    for svc in &model.services {
        if svc.restart.as_deref().unwrap_or("no") == "no" {
            findings.push(
                Finding::new("readiness.no_restart_policy", Severity::Warning, "No restart policy")
                    .with_target(&svc.name)
                    .with_remediation("Set `restart: unless-stopped` (or `always`) so the service recovers after crashes/reboots."),
            );
        }
        if !svc.has_healthcheck {
            findings.push(
                Finding::new("readiness.no_healthcheck", Severity::Info, "No healthcheck")
                    .with_target(&svc.name)
                    .with_remediation("Add a `healthcheck:` so unhealthy containers are detected and restarted."),
            );
        }
        if svc.memory_mb.is_none() {
            findings.push(
                Finding::new("readiness.no_memory_limit", Severity::Info, "No memory limit")
                    .with_target(&svc.name)
                    .with_remediation("Set `deploy.resources.limits.memory` to prevent one service starving the host."),
            );
        }
    }

    build_report("readiness", findings, cta())
}

fn cta() -> Option<Cta> {
    Some(Cta {
        label: "Make it production-ready and deploy on TryDirect →".to_string(),
        url: "https://try.direct/deploy".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_service_flags_operational_gaps() {
        let yaml = "services:\n  web:\n    image: nginx:1.27-alpine\n";
        let r = audit_readiness(yaml);
        assert!(r.findings.iter().any(|f| f.id == "readiness.no_restart_policy"));
        assert!(r.findings.iter().any(|f| f.id == "readiness.no_healthcheck"));
        assert!(r.findings.iter().any(|f| f.id == "readiness.no_memory_limit"));
        assert_eq!(r.checker, "readiness");
    }

    #[test]
    fn well_configured_service_is_ready() {
        let yaml = r#"
services:
  web:
    image: nginx:1.27-alpine
    restart: unless-stopped
    ports: ["80:80"]
    deploy: { resources: { limits: { cpus: "0.5", memory: 128M } } }
    healthcheck:
      test: ["CMD", "wget", "-q", "http://localhost/"]
"#;
        let r = audit_readiness(yaml);
        assert!(
            r.findings.iter().all(|f| f.severity != Severity::Critical),
            "got: {:?}",
            r.findings
        );
        assert!(r.score >= 90, "score {}", r.score);
    }

    #[test]
    fn security_and_exposure_roll_up() {
        // Public postgres + hardcoded secret should surface in the composite.
        let yaml = r#"
services:
  db:
    image: postgres:16
    ports: ["5432:5432"]
    environment:
      API_KEY: "abcd1234efgh5678ijklmnop"
"#;
        let r = audit_readiness(yaml);
        assert!(r.findings.iter().any(|f| f.id == "exposure.sensitive_port_public"));
        assert!(r.findings.iter().any(|f| f.id == "compose.no_secrets"));
    }
}
