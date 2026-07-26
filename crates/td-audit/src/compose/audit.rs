//! Public Compose Auditor (#1): parse a `docker-compose.yml`, run the shared
//! security scanner, and map the result into the uniform [`AuditReport`].

use crate::schema::{AuditReport, Cta, Finding, Severity};
use crate::score::build_report;

use super::validator::{validate_stack_security, SecurityCheckResult, SecurityReport};

/// Audit a raw `docker-compose.yml` string.
///
/// Invalid YAML yields a single critical finding rather than an error, so the
/// public endpoint always returns a graded report.
pub fn audit_compose(yaml: &str) -> AuditReport {
    let value: serde_json::Value = match serde_yaml::from_str(yaml) {
        Ok(v) => v,
        Err(err) => {
            let finding = Finding::new(
                "compose.invalid_yaml",
                Severity::Critical,
                "File is not valid YAML",
            )
            .with_detail(err.to_string())
            .with_remediation("Fix the YAML syntax so the compose file can be parsed.");
            return build_report("compose", vec![finding], compose_cta());
        }
    };

    let report = validate_stack_security(&value);
    build_report("compose", report_to_findings(&report), compose_cta())
}

/// Map one named security check into a [`Finding`] (skipped when it passed).
fn check_to_finding(id: &str, check: &SecurityCheckResult) -> Option<Finding> {
    if check.passed {
        return None;
    }
    let severity = match check.severity.as_str() {
        "critical" => Severity::Critical,
        "warning" => Severity::Warning,
        _ => Severity::Info,
    };
    let mut finding = Finding::new(id, severity, check.message.clone());
    if !check.details.is_empty() {
        finding = finding.with_detail(check.details.join("; "));
    }
    Some(finding)
}

fn report_to_findings(report: &SecurityReport) -> Vec<Finding> {
    [
        ("compose.no_secrets", &report.no_secrets),
        ("compose.no_hardcoded_creds", &report.no_hardcoded_creds),
        ("compose.valid_docker_syntax", &report.valid_docker_syntax),
        ("compose.no_malicious_code", &report.no_malicious_code),
        ("compose.hardened_images", &report.hardened_images),
    ]
    .into_iter()
    .filter_map(|(id, check)| check_to_finding(id, check))
    .collect()
}

fn compose_cta() -> Option<Cta> {
    Some(Cta {
        label: "Fix these and deploy on TryDirect →".to_string(),
        url: "https://try.direct/deploy".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Grade;

    const INSECURE: &str = include_str!("../../tests/fixtures/compose/insecure.yml");
    const CLEAN: &str = include_str!("../../tests/fixtures/compose/clean.yml");

    #[test]
    fn insecure_compose_flags_secret_and_fails() {
        let report = audit_compose(INSECURE);
        assert_eq!(report.checker, "compose");
        // Hardcoded secret is a critical finding from the no_secrets check.
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.id == "compose.no_secrets" && f.severity == Severity::Critical),
            "expected a critical no_secrets finding, got: {:?}",
            report.findings
        );
        assert_eq!(report.grade, Grade::F);
        assert!(report.cta.is_some());
    }

    #[test]
    fn clean_compose_has_no_critical_findings() {
        let report = audit_compose(CLEAN);
        assert!(
            report.findings.iter().all(|f| f.severity != Severity::Critical),
            "clean compose should have no critical findings, got: {:?}",
            report.findings
        );
        assert!(report.score >= 80, "clean compose should score well, got {}", report.score);
    }

    #[test]
    fn invalid_yaml_yields_one_critical_finding_not_an_error() {
        let report = audit_compose(":\n  this is not: valid: yaml: [");
        assert_eq!(report.grade, Grade::F);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].id, "compose.invalid_yaml");
        assert_eq!(report.findings[0].severity, Severity::Critical);
    }

    #[test]
    fn passing_checks_produce_no_finding() {
        let passed = SecurityCheckResult {
            passed: true,
            severity: "info".to_string(),
            message: "ok".to_string(),
            details: vec![],
        };
        assert!(check_to_finding("compose.no_secrets", &passed).is_none());
    }
}
