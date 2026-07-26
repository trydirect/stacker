//! Docker Image Inspector (#3).
//!
//! The grading engine [`audit_image`] is a pure function over already-resolved
//! data ([`ImageInfo`] + [`Vulnerability`] list), so it unit-tests without any
//! network. The server fetches that data through the async [`ImageMetadata`] /
//! [`VulnScanner`] traits (e.g. DockerHub + Trivy) and then calls the engine.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::schema::{AuditReport, Cta, Finding, Severity};
use crate::score::build_report;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageInfo {
    pub reference: String,
    pub exists: bool,
    /// Official (`library/*`) or a verified publisher.
    pub official: bool,
    /// Pinned to an explicit non-`latest` tag or a digest.
    pub pinned: bool,
    /// Days since the image was last pushed, if known.
    pub last_updated_days: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VulnSeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vulnerability {
    pub id: String,
    pub severity: VulnSeverity,
}

/// Fetches registry metadata for an image reference.
#[async_trait]
pub trait ImageMetadata {
    async fn fetch(&self, image: &str) -> Result<ImageInfo, String>;
}

/// Scans an image reference for known vulnerabilities (e.g. Trivy/Grype).
#[async_trait]
pub trait VulnScanner {
    async fn scan(&self, image: &str) -> Result<Vec<Vulnerability>, String>;
}

const STALE_DAYS: u64 = 365;

/// Grade an image from resolved metadata + vulnerabilities.
pub fn audit_image(info: &ImageInfo, vulns: &[Vulnerability]) -> AuditReport {
    let mut findings = Vec::new();

    if !info.exists {
        findings.push(
            Finding::new("image.not_found", Severity::Critical, "Image not found on the registry")
                .with_target(&info.reference)
                .with_remediation("Check the name/namespace/tag; public images must exist and be pullable without auth."),
        );
        // Nothing else is meaningful for a missing image.
        return build_report("image", findings, cta());
    }

    if !info.pinned {
        findings.push(
            Finding::new("image.unpinned_tag", Severity::Warning, "Image uses an unpinned tag (`latest` or none)")
                .with_target(&info.reference)
                .with_remediation("Pin a specific version tag, ideally a digest, for reproducible pulls."),
        );
    }

    if !info.official {
        findings.push(
            Finding::new("image.unverified_publisher", Severity::Info, "Not an official or verified image")
                .with_target(&info.reference)
                .with_remediation("Prefer official (`library/*`) or verified-publisher images where possible."),
        );
    }

    if info.last_updated_days.map(|d| d > STALE_DAYS).unwrap_or(false) {
        findings.push(
            Finding::new("image.stale", Severity::Info, "Image has not been updated in over a year")
                .with_target(&info.reference)
                .with_remediation("Check for a maintained tag; stale images accumulate unpatched CVEs."),
        );
    }

    let count = |sev: VulnSeverity| vulns.iter().filter(|v| v.severity == sev).count();
    let (crit, high, med) = (count(VulnSeverity::Critical), count(VulnSeverity::High), count(VulnSeverity::Medium));
    if crit > 0 {
        findings.push(
            Finding::new("image.vulns_critical", Severity::Critical, format!("{crit} critical vulnerabilit{}", plural(crit)))
                .with_target(&info.reference)
                .with_remediation("Rebuild on a patched base image or a slimmer/hardened variant."),
        );
    }
    if high > 0 {
        findings.push(
            Finding::new("image.vulns_high", Severity::Warning, format!("{high} high-severity vulnerabilit{}", plural(high)))
                .with_target(&info.reference),
        );
    }
    if med > 0 {
        findings.push(
            Finding::new("image.vulns_medium", Severity::Info, format!("{med} medium-severity vulnerabilit{}", plural(med)))
                .with_target(&info.reference),
        );
    }

    build_report("image", findings, cta())
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        "y"
    } else {
        "ies"
    }
}

fn cta() -> Option<Cta> {
    Some(Cta {
        label: "Deploy a patched image on TryDirect →".to_string(),
        url: "https://try.direct/deploy".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Grade;
    use VulnSeverity::*;

    fn info(exists: bool, official: bool, pinned: bool) -> ImageInfo {
        ImageInfo {
            reference: "library/redis:7-alpine".into(),
            exists,
            official,
            pinned,
            last_updated_days: Some(10),
        }
    }

    #[test]
    fn missing_image_is_critical_and_short_circuits() {
        let r = audit_image(&info(false, true, true), &[]);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].id, "image.not_found");
        assert_eq!(r.grade, Grade::F);
    }

    #[test]
    fn clean_official_pinned_image_passes() {
        let r = audit_image(&info(true, true, true), &[]);
        assert!(r.findings.is_empty(), "got {:?}", r.findings);
        assert_eq!(r.grade, Grade::A);
    }

    #[test]
    fn unpinned_and_unofficial_are_flagged() {
        let r = audit_image(&info(true, false, false), &[]);
        assert!(r.findings.iter().any(|f| f.id == "image.unpinned_tag" && f.severity == Severity::Warning));
        assert!(r.findings.iter().any(|f| f.id == "image.unverified_publisher" && f.severity == Severity::Info));
    }

    #[test]
    fn critical_cve_makes_it_fail() {
        let vulns = vec![
            Vulnerability { id: "CVE-1".into(), severity: Critical },
            Vulnerability { id: "CVE-2".into(), severity: High },
            Vulnerability { id: "CVE-3".into(), severity: Medium },
        ];
        let r = audit_image(&info(true, true, true), &vulns);
        assert!(r.findings.iter().any(|f| f.id == "image.vulns_critical" && f.severity == Severity::Critical));
        assert!(r.findings.iter().any(|f| f.id == "image.vulns_high"));
        assert_eq!(r.grade, Grade::F);
    }

    #[test]
    fn stale_image_gets_info() {
        let mut i = info(true, true, true);
        i.last_updated_days = Some(400);
        let r = audit_image(&i, &[]);
        assert!(r.findings.iter().any(|f| f.id == "image.stale" && f.severity == Severity::Info));
    }
}
