//! P1 backfill: re-gate the EXISTING marketplace catalog.
//!
//! The publish gate (`routes::marketplace::creator`) only runs at submit time, so
//! templates approved before it shipped can still carry the author's literal
//! secret values and never regenerate per buyer. This one-shot pass walks the
//! latest version of every template and, for each secret-shaped env key that is
//! not already declared `generated`, (a) attaches a `generated` policy under the
//! key's own service and (b) strips the author's value from the stored
//! `stack_definition` (reusing the publish-time strip). The installer then
//! regenerates per buyer; a value that somehow isn't regenerated is empty
//! (fail-closed), never the author's secret.
//!
//! Dry-run by default: `run(pool, dry_run=true)` reports what WOULD change and
//! mutates nothing. Only `dry_run=false` writes.
//!
//! Scope: YAML compose definitions are handled service-aware. JSON/ProjectForm
//! definitions are reported as `needs_manual` and left untouched — their service
//! mapping is not reliable enough to auto-place a policy, and stripping without a
//! matching policy would break installs.

use std::collections::{BTreeMap, BTreeSet};

use sqlx::PgPool;
use uuid::Uuid;

use crate::cli::config_parser::{ConfigContract, FieldPolicy, Mutability};
use crate::console::commands::cli::init::is_secret_env_key;

#[derive(Debug, sqlx::FromRow)]
struct VersionRow {
    id: Uuid,
    slug: String,
    stack_definition: serde_json::Value,
    definition_format: Option<String>,
    config_contract: Option<serde_json::Value>,
}

#[derive(Debug, Default)]
pub struct BackfillReport {
    pub scanned: usize,
    pub changed: Vec<ChangedTemplate>,
    pub needs_manual: Vec<ManualTemplate>,
}

#[derive(Debug)]
pub struct ChangedTemplate {
    pub slug: String,
    pub version_id: Uuid,
    /// `(service, KEY)` pairs that gained a generated policy + had their value stripped.
    pub added: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct ManualTemplate {
    pub slug: String,
    pub version_id: Uuid,
    pub reason: String,
    pub secret_shaped_keys: Vec<String>,
}

/// Collect `service -> {env keys}` from a YAML compose document.
fn yaml_service_env_keys(yaml_text: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(yaml_text) else {
        return out;
    };
    let Some(services) = doc.get("services").and_then(|s| s.as_mapping()) else {
        return out;
    };
    for (svc_name, svc_val) in services {
        let (Some(name), Some(env)) = (svc_name.as_str(), svc_val.get("environment")) else {
            continue;
        };
        let keys = out.entry(name.to_string()).or_default();
        match env {
            serde_yaml::Value::Mapping(map) => {
                for (k, _) in map {
                    if let Some(k) = k.as_str() {
                        keys.insert(k.to_string());
                    }
                }
            }
            serde_yaml::Value::Sequence(seq) => {
                for item in seq {
                    if let Some(entry) = item.as_str() {
                        if let Some(eq) = entry.find('=') {
                            keys.insert(entry[..eq].to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Compute the augmented contract + stripped definition for one version.
/// Returns `Ok(None)` when nothing needs changing, `Err(reason)` when the
/// definition can't be handled automatically (JSON/ProjectForm).
pub fn plan_version(
    stack_definition: &serde_json::Value,
    definition_format: Option<&str>,
    config_contract: &serde_json::Value,
) -> Result<Option<(serde_json::Value, serde_json::Value, Vec<(String, String)>)>, Vec<String>> {
    let mut contract: ConfigContract =
        serde_json::from_value(config_contract.clone()).unwrap_or_default();

    if definition_format != Some("yaml") {
        // JSON/ProjectForm — flag secret-shaped keys for manual review, don't guess.
        let mut manual = Vec::new();
        // best-effort flat scan for reporting only
        collect_flat_secret_keys(stack_definition, &mut manual);
        let declared: BTreeSet<String> = contract
            .services
            .values()
            .flat_map(|t| t.secret_keys())
            .collect();
        manual.retain(|k| !declared.contains(k));
        if manual.is_empty() {
            return Ok(None);
        }
        return Err(manual);
    }

    let Some(yaml_text) = stack_definition.as_str() else {
        return Ok(None);
    };

    let per_service = yaml_service_env_keys(yaml_text);
    let mut added: Vec<(String, String)> = Vec::new();

    for (svc, keys) in &per_service {
        for key in keys {
            if !is_secret_env_key(key) {
                continue;
            }
            let already = contract
                .services
                .get(svc)
                .and_then(|t| t.fields.get(key))
                .map(|p| p.mutability == Mutability::Generated)
                .unwrap_or(false);
            if already {
                continue;
            }
            contract
                .services
                .entry(svc.clone())
                .or_default()
                .fields
                .insert(key.clone(), FieldPolicy::default_generated_secret());
            added.push((svc.clone(), key.clone()));
        }
    }

    if added.is_empty() {
        return Ok(None);
    }

    let new_contract = serde_json::to_value(&contract).map_err(|e| vec![e.to_string()])?;
    // Reuse the publish-time strip so at-rest author secrets are removed too.
    let new_definition = crate::routes::marketplace::creator::strip_generated_field_values(
        stack_definition,
        definition_format,
        &new_contract,
    );
    Ok(Some((new_contract, new_definition, added)))
}

fn collect_flat_secret_keys(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(name) = map.get("key").and_then(|v| v.as_str()) {
                if map.contains_key("value") && is_secret_env_key(name) {
                    out.push(name.to_string());
                    return;
                }
            }
            for (k, v) in map {
                if (k == "environment" || k == "env") && v.is_object() {
                    if let Some(env) = v.as_object() {
                        for ek in env.keys() {
                            if is_secret_env_key(ek) {
                                out.push(ek.clone());
                            }
                        }
                    }
                }
                collect_flat_secret_keys(v, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                collect_flat_secret_keys(item, out);
            }
        }
        _ => {}
    }
}

/// Walk the latest version of every template; plan (and, unless `dry_run`, apply)
/// the backfill.
pub async fn run(pool: &PgPool, dry_run: bool) -> Result<BackfillReport, String> {
    let rows = sqlx::query_as::<_, VersionRow>(
        r#"SELECT stv.id, st.slug, stv.stack_definition, stv.definition_format, stv.config_contract
           FROM stack_template_version stv
           JOIN stack_template st ON st.id = stv.template_id
           WHERE stv.is_latest = true"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("failed to list versions: {e}"))?;

    let mut report = BackfillReport {
        scanned: rows.len(),
        ..Default::default()
    };

    for row in rows {
        let contract = row
            .config_contract
            .clone()
            .unwrap_or(serde_json::Value::Null);
        match plan_version(
            &row.stack_definition,
            row.definition_format.as_deref(),
            &contract,
        ) {
            Ok(None) => {}
            Ok(Some((new_contract, new_definition, added))) => {
                if !dry_run {
                    sqlx::query(
                        r#"UPDATE stack_template_version
                           SET stack_definition = $2, config_contract = $3
                           WHERE id = $1"#,
                    )
                    .bind(row.id)
                    .bind(&new_definition)
                    .bind(&new_contract)
                    .execute(pool)
                    .await
                    .map_err(|e| format!("update {} failed: {e}", row.id))?;
                }
                report.changed.push(ChangedTemplate {
                    slug: row.slug,
                    version_id: row.id,
                    added,
                });
            }
            Err(manual) => report.needs_manual.push(ManualTemplate {
                slug: row.slug,
                version_id: row.id,
                reason: "non-yaml definition; service mapping not auto-resolved".to_string(),
                secret_shaped_keys: manual,
            }),
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plans_yaml_augment_and_strip_per_service() {
        let yaml = "services:\n  auth:\n    environment:\n      JWT_SECRET: author-secret\n      LOG_LEVEL: warning\n  db:\n    environment:\n      POSTGRES_PASSWORD: author-pw\n";
        let def = json!(yaml);
        let (contract, stripped, added) =
            plan_version(&def, Some("yaml"), &serde_json::Value::Null)
                .expect("plannable")
                .expect("has changes");

        // both secret-shaped keys get a generated policy under their own service
        let added_keys: BTreeSet<(String, String)> = added.into_iter().collect();
        assert!(added_keys.contains(&("auth".to_string(), "JWT_SECRET".to_string())));
        assert!(added_keys.contains(&("db".to_string(), "POSTGRES_PASSWORD".to_string())));

        // stored definition no longer carries the author values
        let text = stripped.as_str().unwrap();
        assert!(!text.contains("author-secret"));
        assert!(!text.contains("author-pw"));
        assert!(text.contains("warning"), "non-secret survives");

        // contract now declares them generated (round-trips through secret_keys)
        let parsed: ConfigContract = serde_json::from_value(contract).unwrap();
        let generated: BTreeSet<String> = parsed
            .services
            .values()
            .flat_map(|t| t.secret_keys())
            .collect();
        assert!(generated.contains("JWT_SECRET"));
        assert!(generated.contains("POSTGRES_PASSWORD"));
    }

    #[test]
    fn noop_when_already_declared() {
        let yaml = "services:\n  auth:\n    environment:\n      JWT_SECRET: x\n";
        let def = json!(yaml);
        let contract = json!({"services":{"auth":{"fields":{"JWT_SECRET":{"mutability":"generated","type":"hex","length":32}}}}});
        assert!(plan_version(&def, Some("yaml"), &contract)
            .unwrap()
            .is_none());
    }

    #[test]
    fn json_definition_reported_as_manual() {
        let def = json!({"services":{"auth":{"environment":{"JWT_SECRET":"x"}}}});
        let err = plan_version(&def, Some("json"), &serde_json::Value::Null).unwrap_err();
        assert_eq!(err, vec!["JWT_SECRET".to_string()]);
    }

    #[test]
    fn noop_when_no_secret_shaped_keys() {
        let yaml =
            "services:\n  app:\n    environment:\n      LOG_LEVEL: info\n      PORT: '8080'\n";
        let def = json!(yaml);
        assert!(plan_version(&def, Some("yaml"), &serde_json::Value::Null)
            .unwrap()
            .is_none());
    }
}
