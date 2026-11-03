//! Scoring — turn a set of [`Finding`]s into a 0..=100 score, a letter
//! [`Grade`], and a finished [`AuditReport`]. Shared by all six checkers so the
//! grade is computed one way everywhere.

use crate::schema::{AuditReport, Cta, Finding, Grade, Severity};

/// 100 minus the summed severity weights, floored at 0.
pub fn score_from_findings(findings: &[Finding]) -> u32 {
    let deduction: u32 = findings.iter().map(|f| f.severity.weight()).sum();
    100u32.saturating_sub(deduction)
}

/// Letter grade for the given findings.
pub fn grade_from_findings(findings: &[Finding]) -> Grade {
    Grade::from_score(score_from_findings(findings))
}

/// One-line human summary, e.g. "1 critical, 2 warnings, 1 info".
pub fn summarize(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "No issues found".to_string();
    }
    let count = |sev: Severity| findings.iter().filter(|f| f.severity == sev).count();
    let mut parts = Vec::new();
    for (sev, singular) in [
        (Severity::Critical, "critical"),
        (Severity::Warning, "warning"),
        (Severity::Info, "info"),
    ] {
        let n = count(sev);
        if n > 0 {
            // "info" is uncountable; "critical"/"warning" pluralize with 's'.
            let label = if n == 1 || singular == "info" {
                singular.to_string()
            } else {
                format!("{singular}s")
            };
            parts.push(format!("{n} {label}"));
        }
    }
    parts.join(", ")
}

/// Assemble the uniform report for a checker from its findings.
pub fn build_report(checker: &str, findings: Vec<Finding>, cta: Option<Cta>) -> AuditReport {
    let score = score_from_findings(&findings);
    AuditReport {
        checker: checker.to_string(),
        score,
        grade: Grade::from_score(score),
        summary: summarize(&findings),
        findings,
        cta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Severity::{Critical, Info, Warning};

    fn f(sev: Severity) -> Finding {
        Finding::new("test.finding", sev, "Test finding")
    }

    #[test]
    fn perfect_when_no_findings() {
        assert_eq!(score_from_findings(&[]), 100);
        assert_eq!(grade_from_findings(&[]), Grade::A);
    }

    #[test]
    fn one_critical_fails() {
        assert_eq!(score_from_findings(&[f(Critical)]), 50);
        assert_eq!(grade_from_findings(&[f(Critical)]), Grade::F);
    }

    #[test]
    fn one_warning_is_b() {
        assert_eq!(score_from_findings(&[f(Warning)]), 85);
        assert_eq!(grade_from_findings(&[f(Warning)]), Grade::B);
    }

    #[test]
    fn info_only_stays_a() {
        assert_eq!(score_from_findings(&[f(Info)]), 97);
        assert_eq!(grade_from_findings(&[f(Info)]), Grade::A);
    }

    #[test]
    fn two_warnings_is_c() {
        assert_eq!(score_from_findings(&[f(Warning), f(Warning)]), 70);
        assert_eq!(grade_from_findings(&[f(Warning), f(Warning)]), Grade::C);
    }

    #[test]
    fn score_floors_at_zero() {
        let many = vec![f(Critical), f(Critical), f(Critical)];
        assert_eq!(score_from_findings(&many), 0);
        assert_eq!(grade_from_findings(&many), Grade::F);
    }

    #[test]
    fn build_report_derives_score_and_grade() {
        let report = build_report("compose", vec![f(Warning)], None);
        assert_eq!(report.checker, "compose");
        assert_eq!(report.score, 85);
        assert_eq!(report.grade, Grade::B);
        assert_eq!(report.findings.len(), 1);
        assert!(report.cta.is_none());
    }

    #[test]
    fn summarize_counts_by_severity() {
        let s = summarize(&[f(Critical), f(Warning), f(Warning), f(Info)]);
        assert!(s.contains("1 critical"), "got: {s}");
        assert!(s.contains("2 warning"), "got: {s}");
        assert!(s.contains("1 info"), "got: {s}");
    }

    #[test]
    fn summarize_all_clear() {
        assert!(summarize(&[]).to_lowercase().contains("no issues"));
    }
}
