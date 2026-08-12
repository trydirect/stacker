//! `resolve_image` — ground truth about a Docker image reference.
//!
//! Pure reference parsing + result assembly live here; the actual registry
//! fetch is injected via [`ImageResolver`] so this unit-tests without network.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use td_audit::image::{audit_image, ImageInfo, Vulnerability};
use td_audit::schema::Grade;

use crate::error::Result;

/// A parsed image reference, normalized to Docker Hub conventions.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRef {
    /// As given, e.g. "redis", "library/redis:7", "ghcr.io/o/r@sha256:..".
    pub reference: String,
    /// Repository path used against the registry API (official → "library/<n>").
    pub repo: String,
    pub tag: Option<String>,
    pub digest: Option<String>,
    /// Official (`library/*`) or bare single-name image.
    pub official: bool,
    /// Pinned to a digest or an explicit non-`latest` tag.
    pub pinned: bool,
}

/// Raw facts a resolver fetched from the registry for a reference.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawImageFacts {
    pub exists: bool,
    pub digest: Option<String>,
    pub size_bytes: Option<u64>,
    pub architectures: Vec<String>,
    pub recent_tags: Vec<String>,
    /// ISO-8601 timestamp of the last push, if known.
    pub last_pushed: Option<String>,
    /// Days since last push (registry-derived), for staleness grading.
    pub last_updated_days: Option<u64>,
}

/// Optional CVE roll-up (from a Trivy scan), included when scanning is enabled.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CveSummary {
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
}

impl CveSummary {
    pub fn from_vulns(vulns: &[Vulnerability]) -> Self {
        use td_audit::image::VulnSeverity::*;
        let count = |s: td_audit::image::VulnSeverity| {
            vulns.iter().filter(|v| v.severity == s).count() as u32
        };
        CveSummary {
            critical: count(Critical),
            high: count(High),
            medium: count(Medium),
            low: count(Low),
        }
    }
}

/// The ground-truth answer returned to the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedImage {
    pub reference: String,
    pub exists: bool,
    pub official: bool,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub architectures: Vec<String>,
    pub recent_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_pushed: Option<String>,
    /// Reuses the td-audit image grade (A–F) so the agent gets a verdict too.
    pub grade: Grade,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cve_summary: Option<CveSummary>,
}

/// Fetches raw registry facts for a repo+tag. Implemented in the server over the
/// Docker Hub client; mocked in tests.
#[async_trait]
pub trait ImageResolver: Send + Sync {
    async fn facts(&self, parsed: &ParsedRef) -> Result<RawImageFacts>;
}

/// Parse and normalize an image reference (Docker Hub conventions).
pub fn parse_image_ref(reference: &str) -> ParsedRef {
    // Peel off a digest first (`name[:tag]@sha256:..`).
    let (name_tag, digest) = match reference.split_once('@') {
        Some((nt, d)) => (nt.to_string(), Some(d.to_string())),
        None => (reference.to_string(), None),
    };

    // A first segment containing '.' or ':' (or "localhost") is a registry host,
    // not a namespace we prefix with "library/".
    let first_seg = name_tag.split('/').next().unwrap_or("");
    let is_registry = name_tag.contains('/')
        && (first_seg.contains('.') || first_seg.contains(':') || first_seg == "localhost");

    // Split a tag only within the last path segment (so registry:port isn't a tag).
    let search_start = name_tag.rfind('/').map(|i| i + 1).unwrap_or(0);
    let (name_part, tag) = match name_tag[search_start..].find(':') {
        Some(rel) => {
            let colon = search_start + rel;
            (name_tag[..colon].to_string(), Some(name_tag[colon + 1..].to_string()))
        }
        None => (name_tag.clone(), None),
    };

    let repo = if name_part.contains('/') {
        name_part.clone()
    } else {
        format!("library/{name_part}")
    };
    let official = !is_registry && (!name_part.contains('/') || name_part.starts_with("library/"));
    let pinned = digest.is_some() || tag.as_deref().map(|t| t != "latest").unwrap_or(false);

    ParsedRef {
        reference: reference.to_string(),
        repo,
        tag,
        digest,
        official,
        pinned,
    }
}

fn expand_cves(c: &CveSummary) -> Vec<Vulnerability> {
    use td_audit::image::VulnSeverity::*;
    let mk = |n: u32, sev| (0..n).map(move |_| Vulnerability { id: "cve".into(), severity: sev });
    mk(c.critical, Critical)
        .chain(mk(c.high, High))
        .chain(mk(c.medium, Medium))
        .chain(mk(c.low, Low))
        .collect()
}

/// Assemble the ground-truth result from parsed ref + fetched facts (+ CVEs).
pub fn assemble(parsed: &ParsedRef, facts: &RawImageFacts, cves: Option<CveSummary>) -> ResolvedImage {
    // Reuse the td-audit image grader so the agent gets a verdict, not just facts.
    let info = ImageInfo {
        reference: parsed.reference.clone(),
        exists: facts.exists,
        official: parsed.official,
        pinned: parsed.pinned,
        last_updated_days: facts.last_updated_days,
    };
    let vulns = cves.as_ref().map(expand_cves).unwrap_or_default();
    let grade = audit_image(&info, &vulns).grade;

    ResolvedImage {
        reference: parsed.reference.clone(),
        exists: facts.exists,
        official: parsed.official,
        pinned: parsed.pinned,
        digest: facts.digest.clone().or_else(|| parsed.digest.clone()),
        size_bytes: facts.size_bytes,
        architectures: facts.architectures.clone(),
        recent_tags: facts.recent_tags.clone(),
        last_pushed: facts.last_pushed.clone(),
        grade,
        cve_summary: cves,
    }
}

/// Orchestrate: parse → fetch facts via the resolver → assemble.
pub async fn resolve_image(
    resolver: &dyn ImageResolver,
    reference: &str,
    cves: Option<CveSummary>,
) -> Result<ResolvedImage> {
    let parsed = parse_image_ref(reference);
    let facts = resolver.facts(&parsed).await?;
    Ok(assemble(&parsed, &facts, cves))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_official_name() {
        let p = parse_image_ref("redis");
        assert_eq!(p.repo, "library/redis");
        assert_eq!(p.tag, None);
        assert!(p.official);
        assert!(!p.pinned); // no tag/digest -> unpinned
    }

    #[test]
    fn parses_official_with_tag() {
        let p = parse_image_ref("redis:7-alpine");
        assert_eq!(p.repo, "library/redis");
        assert_eq!(p.tag.as_deref(), Some("7-alpine"));
        assert!(p.official);
        assert!(p.pinned); // explicit non-latest tag
    }

    #[test]
    fn latest_tag_is_not_pinned() {
        let p = parse_image_ref("redis:latest");
        assert_eq!(p.tag.as_deref(), Some("latest"));
        assert!(!p.pinned);
    }

    #[test]
    fn parses_namespaced_image() {
        let p = parse_image_ref("louislam/dockge:1.4");
        assert_eq!(p.repo, "louislam/dockge");
        assert_eq!(p.tag.as_deref(), Some("1.4"));
        assert!(!p.official);
        assert!(p.pinned);
    }

    #[test]
    fn parses_digest_pinned() {
        let p = parse_image_ref("nginx@sha256:abc123");
        assert_eq!(p.repo, "library/nginx");
        assert_eq!(p.digest.as_deref(), Some("sha256:abc123"));
        assert_eq!(p.tag, None);
        assert!(p.pinned); // digest is pinned
    }

    #[test]
    fn registry_qualified_ref_keeps_host() {
        // A host with a dot is a registry, not a namespace to prefix with library/.
        let p = parse_image_ref("ghcr.io/owner/repo:v2");
        assert_eq!(p.repo, "ghcr.io/owner/repo");
        assert_eq!(p.tag.as_deref(), Some("v2"));
        assert!(!p.official);
    }

    #[test]
    fn registry_with_port_not_split_as_tag() {
        let p = parse_image_ref("localhost:5000/myimg");
        assert_eq!(p.repo, "localhost:5000/myimg");
        assert_eq!(p.tag, None);
    }

    #[test]
    fn assemble_merges_facts_and_grades() {
        let parsed = parse_image_ref("library/redis:7-alpine");
        let facts = RawImageFacts {
            exists: true,
            digest: Some("sha256:deadbeef".into()),
            size_bytes: Some(12_000_000),
            architectures: vec!["amd64".into(), "arm64".into()],
            recent_tags: vec!["7-alpine".into(), "7".into()],
            last_pushed: Some("2026-06-01T00:00:00Z".into()),
            last_updated_days: Some(30),
        };
        let r = assemble(&parsed, &facts, None);
        assert!(r.exists);
        assert_eq!(r.architectures, vec!["amd64", "arm64"]);
        assert_eq!(r.size_bytes, Some(12_000_000));
        assert!(r.official && r.pinned);
        // Official, pinned, recent, no CVEs -> clean grade A.
        assert_eq!(r.grade, Grade::A);
    }

    #[test]
    fn missing_image_grades_f() {
        let parsed = parse_image_ref("trydirect/does-not-exist:latest");
        let facts = RawImageFacts { exists: false, ..Default::default() };
        let r = assemble(&parsed, &facts, None);
        assert!(!r.exists);
        assert_eq!(r.grade, Grade::F);
    }

    // A mock resolver proves the orchestrator wiring without network.
    struct MockResolver(RawImageFacts);
    #[async_trait]
    impl ImageResolver for MockResolver {
        async fn facts(&self, _p: &ParsedRef) -> Result<RawImageFacts> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn resolve_image_orchestrates_parse_fetch_assemble() {
        let resolver = MockResolver(RawImageFacts {
            exists: true,
            architectures: vec!["amd64".into()],
            last_updated_days: Some(5),
            ..Default::default()
        });
        let r = resolve_image(&resolver, "redis:7", None).await.unwrap();
        assert_eq!(r.reference, "redis:7");
        assert!(r.exists);
        assert!(r.pinned);
    }
}
