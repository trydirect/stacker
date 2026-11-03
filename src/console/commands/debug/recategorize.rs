use crate::configuration::get_configuration;
use actix_web::{rt, Result};
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Applies `category_mapping.csv` (produced by
/// `user/scripts/generate_category_mapping.py`) to the Stacker database:
///
/// - resolves/creates canonical `stack_category` rows (renaming legacy
///   duplicates like `Data Sciense`, `crm`, `ai-agents` into their canonical
///   spelling),
/// - re-points `stack_template.category_id` and rewrites `stack_template.tags`
///   per the mapping,
/// - deletes unreferenced duplicate category rows.
///
/// Usage:
///     console debug recategorize --map category_mapping.csv --dry-run
///     console debug recategorize --map category_mapping.csv
pub struct RecategorizeCommand {
    map_path: String,
    dry_run: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MappingRow {
    pub slug: String,
    pub old_category: String,
    pub new_category: String,
    pub tags: Vec<String>,
}

/// Merge duplicate/misspelled category spellings into the canonical one.
/// Identity for every canonical name.
pub fn canonical_category(name: &str) -> String {
    let normalized = name
        .trim()
        .to_lowercase()
        .replace('_', " ")
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let canonical = match normalized.as_str() {
        "ai" | "ai agents" | "artificial intelligence" => "AI",
        "data sciense" | "data science" => "Data Science",
        "machine learning" | "machinelearning" | "ml" => "Machine Learning",
        "crm" => "CRM",
        "e commerce" | "ecommerce" => "E-commerce",
        "development" | "dev" => "Development",
        "cms" => "CMS",
        "collaboration" => "Collaboration",
        "project management" | "projectmanagement" => "Project Management",
        "security" | "cyber security" | "cybersecurity" => "Cyber Security",
        "hosting platform" | "hosting" => "Hosting platform",
        "marketing automation" | "marketingautomation" => "Marketing Automation",
        "project generator" | "projectgenerator" => "Project Generator",
        "teamwork" => "Teamwork",
        "education" => "Education",
        "erp" => "ERP",
        "game" | "gaming" => "Game",
        "big data" | "bigdata" => "Big Data",
        "blockchain" => "Blockchain",
        "new" => "New",
        _ => name.trim(),
    };
    canonical.to_string()
}

/// True when `name` is a non-canonical spelling that the merge should remove.
pub fn is_duplicate_spelling(name: &str) -> bool {
    canonical_category(name) != name.trim()
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// Parses the generator CSV. Columns are located by header name, so column
/// order changes don't break the tool.
pub fn parse_mapping_csv(content: &str) -> Result<Vec<MappingRow>, String> {
    let mut rows: Vec<MappingRow> = Vec::new();
    let mut header: Option<Vec<String>> = None;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_csv_line(line);
        if header.is_none() {
            let lower: Vec<String> = fields.iter().map(|f| f.to_lowercase()).collect();
            if !lower.iter().any(|f| f == "slug") {
                return Err("CSV header must contain a 'slug' column".to_string());
            }
            header = Some(lower);
            continue;
        }
        let cols = header.as_ref().unwrap();
        let get = |name: &str| -> String {
            cols.iter()
                .position(|c| c == name)
                .and_then(|idx| fields.get(idx))
                .cloned()
                .unwrap_or_default()
        };
        let slug = get("slug");
        if slug.is_empty() {
            continue;
        }
        rows.push(MappingRow {
            slug,
            old_category: get("old_category"),
            new_category: get("new_category"),
            tags: get("tags")
                .split_whitespace()
                .map(|t| t.to_string())
                .collect(),
        });
    }

    if rows.is_empty() {
        return Err("CSV contains no data rows".to_string());
    }
    Ok(rows)
}

impl RecategorizeCommand {
    pub fn new(map_path: String, dry_run: bool) -> Self {
        Self { map_path, dry_run }
    }
}

impl crate::console::commands::CallableTrait for RecategorizeCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        rt::System::new().block_on(async {
            let content = std::fs::read_to_string(&self.map_path)
                .map_err(|e| format!("Cannot read mapping file {}: {}", self.map_path, e))?;
            let rows = parse_mapping_csv(&content)?;

            let settings = get_configuration().map_err(|e| format!("Configuration: {:?}", e))?;
            let pool = PgPool::connect(&settings.database.connection_string())
                .await
                .map_err(|e| format!("Failed to connect to database: {}", e))?;

            run_recategorize(&pool, &rows, self.dry_run).await?;
            Ok(())
        })
    }
}

async fn run_recategorize(
    pool: &PgPool,
    rows: &[MappingRow],
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;

    // Existing category rows.
    let existing: Vec<(i32, String)> = sqlx::query_as("SELECT id, name FROM stack_category")
        .fetch_all(&mut *tx)
        .await?;
    let mut by_name: HashMap<String, i32> = HashMap::new();
    for (id, name) in &existing {
        by_name.insert(name.clone(), *id);
    }

    // Distinct target categories from the mapping.
    let targets: BTreeSet<String> = rows
        .iter()
        .map(|r| canonical_category(&r.new_category))
        .collect();

    // Resolve a row id for every target: exact name, else rename the single
    // duplicate spelling, else insert a fresh canonical row.
    let mut target_ids: BTreeMap<String, i32> = BTreeMap::new();
    for target in &targets {
        if let Some(id) = by_name.get(target) {
            target_ids.insert(target.clone(), *id);
            continue;
        }
        let candidates: Vec<(i32, String)> = existing
            .iter()
            .filter(|(_, name)| {
                name.as_str() != target.as_str()
                    && canonical_category(name) == canonical_category(target)
            })
            .cloned()
            .collect();
        if candidates.len() == 1 {
            let (id, old_name) = &candidates[0];
            println!(
                "[category] rename '{}' -> '{}' (id={})",
                old_name, target, id
            );
            if !dry_run {
                sqlx::query(
                    "UPDATE stack_category SET name = $1, title = COALESCE(title, $1) WHERE id = $2",
                )
                .bind(target)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            }
            target_ids.insert(target.clone(), *id);
        } else {
            println!("[category] insert '{}' (new row)", target);
            if !dry_run {
                let id: i32 = sqlx::query_scalar(
                    "INSERT INTO stack_category (name, title) VALUES ($1, $1) RETURNING id",
                )
                .bind(target)
                .fetch_one(&mut *tx)
                .await?;
                target_ids.insert(target.clone(), id);
            } else {
                target_ids.insert(target.clone(), -1);
            }
        }
    }

    // Re-point templates and rewrite tags.
    let mut updated = 0usize;
    let mut unchanged = 0usize;
    let mut missing = 0usize;
    for row in rows {
        let target = canonical_category(&row.new_category);
        let Some(&category_id) = target_ids.get(&target) else {
            missing += 1;
            continue;
        };
        let current: Option<(Option<i32>, serde_json::Value)> = sqlx::query_as(
            "SELECT category_id, tags FROM stack_template WHERE slug = $1",
        )
        .bind(&row.slug)
        .fetch_optional(&mut *tx)
        .await?;

        let Some((current_category_id, current_tags)) = current else {
            println!("[template] '{}' not found in stack_template — skipped", row.slug);
            missing += 1;
            continue;
        };

        let new_tags = serde_json::Value::Array(
            row.tags
                .iter()
                .map(|t| serde_json::Value::String(t.clone()))
                .collect(),
        );
        let tags_changed = current_tags != new_tags;
        let category_changed = current_category_id != Some(category_id);

        if !tags_changed && !category_changed {
            unchanged += 1;
            continue;
        }

        updated += 1;
        println!(
            "[template] '{}': category {} -> {}, tags {:?} -> {:?}",
            row.slug,
            current_category_id.map(|i| i.to_string()).unwrap_or_else(|| "NULL".into()),
            category_id,
            current_tags,
            new_tags
        );
        if !dry_run {
            sqlx::query(
                "UPDATE stack_template SET category_id = $1, tags = $2, updated_at = now() WHERE slug = $3",
            )
            .bind(category_id)
            .bind(&new_tags)
            .bind(&row.slug)
            .execute(&mut *tx)
            .await?;
        }
    }

    // Drop unreferenced duplicate-spelling category rows (crm, ai-agents, ...).
    let referenced: BTreeSet<i32> = {
        let ids: Vec<(Option<i32>,)> = sqlx::query_as("SELECT category_id FROM stack_template")
            .fetch_all(&mut *tx)
            .await?;
        ids.into_iter().filter_map(|(id,)| id).collect()
    };
    let mut removed = 0usize;
    for (id, name) in &existing {
        if referenced.contains(id) {
            continue;
        }
        if is_duplicate_spelling(name) {
            println!("[category] delete unreferenced duplicate '{}' (id={})", name, id);
            removed += 1;
            if !dry_run {
                sqlx::query("DELETE FROM stack_category WHERE id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }

    println!(
        "\nSummary: {} templates updated, {} unchanged, {} missing, {} category rows created/renamed, {} duplicates removed (dry_run={})",
        updated,
        unchanged,
        missing,
        target_ids.len(),
        removed,
        dry_run
    );

    if dry_run {
        tx.rollback().await?;
        println!("Dry run — all changes rolled back.");
    } else {
        tx.commit().await?;
        println!("Changes committed.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_merges_known_duplicates() {
        assert_eq!(canonical_category("ai-agents"), "AI");
        assert_eq!(canonical_category("AI"), "AI");
        assert_eq!(canonical_category("Data Sciense"), "Data Science");
        assert_eq!(canonical_category("crm"), "CRM");
        assert_eq!(canonical_category("project_management"), "Project Management");
        assert_eq!(canonical_category("project-management"), "Project Management");
        assert_eq!(canonical_category("e-commerce"), "E-commerce");
    }

    #[test]
    fn canonical_keeps_canonical_names_identity() {
        for name in [
            "AI",
            "Cyber Security",
            "Data Science",
            "Machine Learning",
            "E-commerce",
            "Hosting platform",
            "file-sharing",
            "devops",
            "databases",
            "communication",
        ] {
            assert_eq!(canonical_category(name), name, "identity broken for {}", name);
        }
    }

    #[test]
    fn duplicate_spelling_detected() {
        assert!(is_duplicate_spelling("crm"));
        assert!(is_duplicate_spelling("ai-agents"));
        assert!(is_duplicate_spelling("Data Sciense"));
        assert!(!is_duplicate_spelling("AI"));
        assert!(!is_duplicate_spelling("devops"));
        assert!(!is_duplicate_spelling("file-sharing"));
    }

    #[test]
    fn parses_generator_csv() {
        let csv = "slug,old_category,new_category,origin,tags\n\
                   open-webui,devops,AI,rule,ai open-webui\n\
                   \"quoted,slug\",devops,CMS,rule, cms wordpress\n";
        let rows = parse_mapping_csv(csv).expect("parses");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].slug, "open-webui");
        assert_eq!(rows[0].new_category, "AI");
        assert_eq!(rows[0].tags, vec!["ai", "open-webui"]);
        assert_eq!(rows[1].slug, "quoted,slug");
        assert_eq!(rows[1].new_category, "CMS");
    }

    #[test]
    fn rejects_csv_without_slug_header() {
        assert!(parse_mapping_csv("foo,bar\n1,2\n").is_err());
    }
}
