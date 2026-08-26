//! Declarative pipe reconciliation — the pure core behind `stacker pipe diff`
//! and `stacker pipe apply` (§5 / #1 of the PIPE IaC plan).
//!
//! Compares the pipes declared in `stacker.yml` (`config.pipes`) against what is
//! deployed, keyed by pipe **name**. This module is deliberately I/O-free so the
//! reconcile logic is unit-testable; the command layer supplies the deployed
//! view (built from the API's pipe templates) and performs create/update.

use crate::cli::config_parser::PipeSpec;

/// A minimal, comparable view of a deployed pipe (built by the command layer
/// from `PipeTemplateInfo`). Endpoints are normalized to "METHOD /path".
#[derive(Debug, Clone, PartialEq)]
pub struct DeployedPipe {
    pub name: String,
    pub source_app: String,
    pub target_app: String,
    pub source_endpoint: String,
    pub target_endpoint: String,
}

/// What a reconcile would do to one pipe.
#[derive(Debug, Clone, PartialEq)]
pub enum PipeAction {
    /// Declared but not deployed → would be created.
    Create,
    /// Declared and deployed but one or more compared fields differ.
    Update { changes: Vec<String> },
    /// Declared and deployed, identical.
    Unchanged,
    /// Deployed but not declared → orphan (candidate for `--prune`).
    Orphan,
}

/// One entry in the reconcile plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PipeDiffEntry {
    pub name: String,
    pub action: PipeAction,
}

/// Normalize a "METHOD /path" (or bare "/path" → GET) endpoint for comparison:
/// uppercased method + trimmed path.
pub fn normalize_endpoint(spec: &str) -> String {
    let trimmed = spec.trim();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("").trim();
    match parts.next().map(str::trim) {
        Some(path) if !path.is_empty() => format!("{} {}", first.to_ascii_uppercase(), path),
        _ => format!("GET {}", first),
    }
}

/// Compute the reconcile plan: for each declared pipe, whether it would be
/// created / updated / left unchanged; plus any deployed pipe not declared
/// (orphan). Deterministic ordering: declared pipes first (in declaration
/// order), then orphans (sorted by name).
pub fn diff_pipes(specs: &[PipeSpec], deployed: &[DeployedPipe]) -> Vec<PipeDiffEntry> {
    let mut plan = Vec::new();

    for spec in specs {
        let entry = match deployed.iter().find(|d| d.name == spec.name) {
            None => PipeDiffEntry {
                name: spec.name.clone(),
                action: PipeAction::Create,
            },
            Some(dep) => {
                let mut changes = Vec::new();
                if dep.source_app != spec.source {
                    changes.push(format!("source: {} → {}", dep.source_app, spec.source));
                }
                if dep.target_app != spec.target {
                    changes.push(format!("target: {} → {}", dep.target_app, spec.target));
                }
                let want_src = normalize_endpoint(&spec.source_endpoint);
                if normalize_endpoint(&dep.source_endpoint) != want_src {
                    changes.push(format!(
                        "source_endpoint: {} → {}",
                        dep.source_endpoint, want_src
                    ));
                }
                let want_tgt = normalize_endpoint(&spec.target_endpoint);
                if normalize_endpoint(&dep.target_endpoint) != want_tgt {
                    changes.push(format!(
                        "target_endpoint: {} → {}",
                        dep.target_endpoint, want_tgt
                    ));
                }
                PipeDiffEntry {
                    name: spec.name.clone(),
                    action: if changes.is_empty() {
                        PipeAction::Unchanged
                    } else {
                        PipeAction::Update { changes }
                    },
                }
            }
        };
        plan.push(entry);
    }

    // Orphans: deployed but not declared.
    let declared: std::collections::HashSet<&str> =
        specs.iter().map(|s| s.name.as_str()).collect();
    let mut orphans: Vec<&DeployedPipe> = deployed
        .iter()
        .filter(|d| !declared.contains(d.name.as_str()))
        .collect();
    orphans.sort_by(|a, b| a.name.cmp(&b.name));
    for dep in orphans {
        plan.push(PipeDiffEntry {
            name: dep.name.clone(),
            action: PipeAction::Orphan,
        });
    }

    plan
}

/// True when the plan has no create/update/orphan work (everything matches).
pub fn plan_is_clean(plan: &[PipeDiffEntry]) -> bool {
    plan.iter()
        .all(|e| matches!(e.action, PipeAction::Unchanged))
}

/// Parse a "METHOD /path" (or bare "/path" → GET) endpoint into the template
/// JSON shape the API expects (`{"method","path"}`).
pub fn endpoint_to_json(spec: &str) -> serde_json::Value {
    let norm = normalize_endpoint(spec);
    let mut parts = norm.splitn(2, ' ');
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");
    serde_json::json!({ "method": method, "path": path })
}

/// Deterministic field mapping for a declared pipe: each target field draws from
/// a same-named source field, else the positionally-aligned source field, else
/// itself. Empty target → empty (pass-through) mapping. (Same rule as the
/// imperative `pipe create --manual` path.)
pub fn field_mapping_for(src: &[String], tgt: &[String]) -> serde_json::Value {
    let mut mapping = serde_json::Map::new();
    for (idx, target_field) in tgt.iter().enumerate() {
        let source_ref = if src.iter().any(|s| s == target_field) {
            target_field.clone()
        } else if let Some(s) = src.get(idx) {
            s.clone()
        } else {
            target_field.clone()
        };
        mapping.insert(
            target_field.clone(),
            serde_json::Value::String(format!("$.{source_ref}")),
        );
    }
    serde_json::Value::Object(mapping)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, src: &str, tgt: &str, se: &str, te: &str) -> PipeSpec {
        PipeSpec {
            name: name.into(),
            source: src.into(),
            target: tgt.into(),
            source_endpoint: se.into(),
            target_endpoint: te.into(),
            source_fields: vec![],
            target_fields: vec![],
            trigger: "manual".into(),
            poll_interval: None,
            retry: None,
            retry_backoff_ms: None,
            retry_backoff_max_ms: None,
            on_failure: None,
            on_success: None,
        }
    }

    fn dep(name: &str, src: &str, tgt: &str, se: &str, te: &str) -> DeployedPipe {
        DeployedPipe {
            name: name.into(),
            source_app: src.into(),
            target_app: tgt.into(),
            source_endpoint: se.into(),
            target_endpoint: te.into(),
        }
    }

    #[test]
    fn normalize_endpoint_uppercases_and_defaults_get() {
        assert_eq!(normalize_endpoint("post /x"), "POST /x");
        assert_eq!(normalize_endpoint("/x"), "GET /x");
        assert_eq!(normalize_endpoint("GET  /x"), "GET /x");
    }

    #[test]
    fn create_update_unchanged_orphan_are_all_detected() {
        let specs = vec![
            spec("new", "a", "b", "GET /s", "POST /t"), // not deployed → create
            spec("same", "a", "b", "GET /s", "POST /t"), // identical → unchanged
            spec("changed", "a", "b2", "GET /s", "POST /t2"), // differs → update
        ];
        let deployed = vec![
            dep("same", "a", "b", "GET /s", "POST /t"),
            dep("changed", "a", "b", "GET /s", "POST /t"), // target + endpoint differ
            dep("gone", "a", "b", "GET /s", "POST /t"),    // not declared → orphan
        ];

        let plan = diff_pipes(&specs, &deployed);
        assert_eq!(plan[0], PipeDiffEntry { name: "new".into(), action: PipeAction::Create });
        assert_eq!(plan[1].action, PipeAction::Unchanged);
        match &plan[2].action {
            PipeAction::Update { changes } => {
                assert!(changes.iter().any(|c| c.contains("target:")));
                assert!(changes.iter().any(|c| c.contains("target_endpoint:")));
            }
            other => panic!("expected update, got {other:?}"),
        }
        assert_eq!(plan[3], PipeDiffEntry { name: "gone".into(), action: PipeAction::Orphan });
        assert!(!plan_is_clean(&plan));
    }

    #[test]
    fn clean_plan_when_all_match() {
        let specs = vec![spec("p", "a", "b", "GET /s", "POST /t")];
        let deployed = vec![dep("p", "a", "b", "GET /s", "POST /t")];
        let plan = diff_pipes(&specs, &deployed);
        assert!(plan_is_clean(&plan));
    }

    #[test]
    fn endpoint_diff_is_method_case_insensitive() {
        // spec "post /t" vs deployed "POST /t" → no change.
        let specs = vec![spec("p", "a", "b", "get /s", "post /t")];
        let deployed = vec![dep("p", "a", "b", "GET /s", "POST /t")];
        assert!(plan_is_clean(&diff_pipes(&specs, &deployed)));
    }

    #[test]
    fn endpoint_to_json_produces_method_path() {
        assert_eq!(
            endpoint_to_json("post /pipetest"),
            serde_json::json!({ "method": "POST", "path": "/pipetest" })
        );
        assert_eq!(
            endpoint_to_json("/status"),
            serde_json::json!({ "method": "GET", "path": "/status" })
        );
    }

    #[test]
    fn field_mapping_for_matches_name_then_position() {
        assert_eq!(
            field_mapping_for(&["message".into()], &["message".into()]),
            serde_json::json!({ "message": "$.message" })
        );
        assert_eq!(
            field_mapping_for(&["body".into()], &["message".into()]),
            serde_json::json!({ "message": "$.body" })
        );
        assert_eq!(
            field_mapping_for(&["x".into()], &[]),
            serde_json::json!({})
        );
    }
}
