//! P1 one-shot: re-gate the existing marketplace catalog.
//!
//! For the latest version of every template, attach a `generated` policy to each
//! secret-shaped env key that lacks one and strip the author's value from the
//! stored `stack_definition`, so existing templates regenerate secrets per buyer
//! instead of shipping the author's. See `helpers::field_policy_backfill`.
//!
//! DRY-RUN by default — prints what would change and writes nothing.
//! Pass `--apply` to actually update rows.
//!
//! Usage:
//!   DATABASE_URL=postgres://… cargo run --bin backfill_field_policy            # dry-run
//!   DATABASE_URL=postgres://… cargo run --bin backfill_field_policy -- --apply # write

use stacker::helpers::field_policy_backfill;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let apply = std::env::args().any(|a| a == "--apply");
    let dry_run = !apply;

    let db_url = std::env::var("DATABASE_URL")
        .map_err(|_| "set DATABASE_URL to the stacker Postgres".to_string())?;
    let pool = sqlx::PgPool::connect(&db_url).await?;

    let report = field_policy_backfill::run(&pool, dry_run)
        .await
        .map_err(|e| e.to_string())?;

    let mode = if dry_run {
        "DRY-RUN (no writes)"
    } else {
        "APPLIED"
    };
    eprintln!("=== field-policy backfill — {mode} ===");
    eprintln!("scanned latest versions: {}", report.scanned);
    eprintln!(
        "templates {}: {}",
        if dry_run {
            "that WOULD change"
        } else {
            "changed"
        },
        report.changed.len()
    );
    for c in &report.changed {
        let keys: Vec<String> = c
            .added
            .iter()
            .map(|(svc, key)| format!("{svc}.{key}"))
            .collect();
        eprintln!("  {} [{}]  +{}", c.slug, c.version_id, keys.join(", "));
    }
    if !report.needs_manual.is_empty() {
        eprintln!(
            "\nNEEDS MANUAL REVIEW (non-YAML definitions, {} template(s)):",
            report.needs_manual.len()
        );
        for m in &report.needs_manual {
            eprintln!(
                "  {} [{}] — {} — secret-shaped: [{}]",
                m.slug,
                m.version_id,
                m.reason,
                m.secret_shaped_keys.join(", ")
            );
        }
    }
    if dry_run {
        eprintln!("\nRe-run with --apply to write these changes.");
    }
    Ok(())
}
