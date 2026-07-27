//! Dockerfile Linter (#2): lightweight instruction scan for common security /
//! reproducibility issues. Pure string analysis, no external tools.

use crate::schema::{AuditReport, Cta, Finding, Severity};
use crate::score::build_report;

/// A logical Dockerfile instruction (line continuations already joined).
struct Instruction {
    keyword: String,
    args: String,
}

fn parse_instructions(dockerfile: &str) -> Vec<Instruction> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for raw in dockerfile.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(stripped) = line.strip_suffix('\\') {
            buf.push_str(stripped.trim_end());
            buf.push(' ');
            continue;
        }
        buf.push_str(line);
        let joined = std::mem::take(&mut buf);
        let mut parts = joined.splitn(2, char::is_whitespace);
        let keyword = parts.next().unwrap_or("").to_uppercase();
        let args = parts.next().unwrap_or("").trim().to_string();
        if !keyword.is_empty() {
            out.push(Instruction { keyword, args });
        }
    }
    out
}

fn base_image_is_unpinned(args: &str) -> bool {
    // Ignore `--platform=` flags and `AS stage` aliases.
    let image = args
        .split_whitespace()
        .find(|t| !t.starts_with("--"))
        .unwrap_or("");
    if image.is_empty() || image.starts_with('$') {
        return false; // build-arg driven; can't judge
    }
    if image.contains('@') {
        return false; // digest-pinned
    }
    match image.rsplit_once(':') {
        Some((_, tag)) => tag.eq_ignore_ascii_case("latest"),
        None => true, // no tag at all
    }
}

/// Audit a raw Dockerfile.
pub fn audit_dockerfile(dockerfile: &str) -> AuditReport {
    let instrs = parse_instructions(dockerfile);
    let mut findings = Vec::new();

    if instrs.is_empty() {
        findings.push(
            Finding::new(
                "dockerfile.empty",
                Severity::Critical,
                "No Dockerfile instructions found",
            )
            .with_remediation("Provide a valid Dockerfile starting with a FROM instruction."),
        );
        return build_report("dockerfile", findings, cta());
    }

    let froms: Vec<&Instruction> = instrs.iter().filter(|i| i.keyword == "FROM").collect();
    for from in &froms {
        if base_image_is_unpinned(&from.args) {
            findings.push(
                Finding::new(
                    "dockerfile.unpinned_base",
                    Severity::Warning,
                    "Base image is unpinned (no tag / `:latest` / no digest)",
                )
                .with_detail(format!("FROM {}", from.args))
                .with_remediation("Pin to a specific version and ideally a digest, e.g. `FROM node:20-alpine@sha256:…`."),
            );
        }
    }

    let has_user_nonroot = instrs
        .iter()
        .any(|i| i.keyword == "USER" && !i.args.trim().is_empty() && i.args.trim() != "root");
    if !has_user_nonroot {
        findings.push(
            Finding::new(
                "dockerfile.root_user",
                Severity::Warning,
                "Container runs as root",
            )
            .with_remediation("Add a non-root `USER` before the entrypoint."),
        );
    }

    // Secrets baked into ENV/ARG.
    for i in instrs
        .iter()
        .filter(|i| i.keyword == "ENV" || i.keyword == "ARG")
    {
        if crate::compose::contains_hardcoded_secret(&i.args) {
            findings.push(
                Finding::new(
                    "dockerfile.secret_in_env",
                    Severity::Critical,
                    "Possible hardcoded secret in ENV/ARG",
                )
                .with_detail(format!("{} {}", i.keyword, mask(&i.args)))
                .with_remediation(
                    "Never bake secrets into images — pass them at runtime or via build secrets.",
                ),
            );
        }
    }

    if instrs
        .iter()
        .any(|i| i.keyword == "ADD" && looks_local(&i.args))
    {
        findings.push(
            Finding::new(
                "dockerfile.add_local",
                Severity::Info,
                "`ADD` used for local files",
            )
            .with_remediation(
                "Prefer `COPY` for local files; reserve `ADD` for remote URLs / tar extraction.",
            ),
        );
    }

    if !instrs.iter().any(|i| i.keyword == "HEALTHCHECK") {
        findings.push(
            Finding::new(
                "dockerfile.no_healthcheck",
                Severity::Info,
                "No HEALTHCHECK defined",
            )
            .with_remediation(
                "Add a `HEALTHCHECK` so orchestrators can detect an unhealthy container.",
            ),
        );
    }

    build_report("dockerfile", findings, cta())
}

fn looks_local(args: &str) -> bool {
    let src = args.split_whitespace().next().unwrap_or("");
    !(src.starts_with("http://") || src.starts_with("https://"))
}

fn mask(s: &str) -> String {
    if s.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}***", &s[..4])
    }
}

fn cta() -> Option<Cta> {
    Some(Cta {
        label: "Harden your image and deploy on TryDirect →".to_string(),
        url: "https://try.direct/deploy".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Grade;

    #[test]
    fn flags_unpinned_base_and_root() {
        let df = "FROM node\nCMD [\"node\", \"app.js\"]\n";
        let r = audit_dockerfile(df);
        assert!(r
            .findings
            .iter()
            .any(|f| f.id == "dockerfile.unpinned_base"));
        assert!(r.findings.iter().any(|f| f.id == "dockerfile.root_user"));
    }

    #[test]
    fn clean_dockerfile_scores_well() {
        let df = "FROM node:20-alpine@sha256:abc\nUSER app\nHEALTHCHECK CMD wget -q localhost || exit 1\nCOPY . /app\n";
        let r = audit_dockerfile(df);
        assert!(r.findings.is_empty(), "got: {:?}", r.findings);
        assert_eq!(r.grade, Grade::A);
    }

    #[test]
    fn secret_in_env_is_critical() {
        let df =
            "FROM alpine:3.20\nUSER app\nHEALTHCHECK CMD true\nENV API_KEY=abcd1234efgh5678ijkl\n";
        let r = audit_dockerfile(df);
        assert!(r
            .findings
            .iter()
            .any(|f| f.id == "dockerfile.secret_in_env" && f.severity == Severity::Critical));
    }

    #[test]
    fn empty_dockerfile_is_critical() {
        let r = audit_dockerfile("# just a comment\n");
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].id, "dockerfile.empty");
    }
}
