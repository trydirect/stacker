use std::path::{Path, PathBuf};

use crate::cli::config_parser::{MarketplacePublishConfig, StackerConfig};
use crate::cli::credentials::CredentialsManager;
use crate::cli::deployment_lock::DeploymentLock;
use crate::cli::error::CliError;
use crate::cli::stacker_client::StackerClient;
use crate::console::commands::CallableTrait;

/// Default config filename.
const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// `stacker submit`
///
/// Packages the current stack project and submits it to the marketplace for
/// review. Reads stacker.yml for project metadata (including the `marketplace:`
/// block), creates or updates the template on the server, then submits it for
/// review. CLI flags override the stacker.yml values.
pub struct SubmitCommand {
    file: Option<String>,
    version: Option<String>,
    slug: Option<String>,
    description: Option<String>,
    long_description: Option<String>,
    long_description_file: Option<String>,
    category: Option<String>,
    plan_type: Option<String>,
    price: Option<f64>,
}

impl SubmitCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        file: Option<String>,
        version: Option<String>,
        slug: Option<String>,
        description: Option<String>,
        long_description: Option<String>,
        long_description_file: Option<String>,
        category: Option<String>,
        plan_type: Option<String>,
        price: Option<f64>,
    ) -> Self {
        Self {
            file,
            version,
            slug,
            description,
            long_description,
            long_description_file,
            category,
            plan_type,
            price,
        }
    }
}

/// CLI values merged with the `marketplace:` block of stacker.yml. Flags win
/// over stacker.yml. `long_description_file` carries the *contents* of
/// `--long-description-file` (already read), so resolution stays pure.
struct SubmitOverrides<'a> {
    slug: Option<&'a str>,
    version: Option<&'a str>,
    short_description: Option<&'a str>,
    long_description: Option<&'a str>,
    long_description_file: Option<&'a str>,
    category: Option<&'a str>,
    plan_type: Option<&'a str>,
    price: Option<f64>,
}

/// Listing metadata sent to the marketplace, resolved from CLI flags and the
/// `marketplace:` block (see `docs/MARKETPLACE_PUBLISH.md`).
#[derive(Debug)]
struct ListingMetadata {
    name: String,
    slug: String,
    version: String,
    short_description: String,
    long_description: String,
    category: Option<String>,
    plan_type: String,
    price: Option<f64>,
    currency: Option<String>,
    tags: Vec<String>,
    vendor_url: Option<String>,
    confirm_no_secrets: bool,
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn listing_hint(field: &str) -> &'static str {
    match field {
        "short_description" => {
            "Set marketplace.short_description in stacker.yml or pass --description."
        }
        _ => {
            "Set marketplace.long_description in stacker.yml, or pass --long-description \
              / --long-description-file."
        }
    }
}

/// Merge CLI flags with the `marketplace:` block and enforce the listing
/// description requirements the marketplace review checks for
/// (`short_description` 20-200 chars, `long_description` 100-5000 chars).
fn resolve_listing(
    config: &StackerConfig,
    overrides: &SubmitOverrides,
) -> Result<ListingMetadata, String> {
    let marketplace: &MarketplacePublishConfig = &config.marketplace;

    if marketplace.publish == Some(false) {
        return Err(
            "marketplace.publish is false in stacker.yml — set it to true (or remove it) to \
             submit this stack to the marketplace"
                .to_string(),
        );
    }
    if marketplace.no_secrets_confirmation == Some(false) {
        return Err(
            "marketplace.no_secrets_confirmation is false in stacker.yml — it must be true \
             before a template enters review"
                .to_string(),
        );
    }

    let name = non_empty(marketplace.display_name.as_deref())
        .unwrap_or(&config.name)
        .to_string();

    let derived_slug = slugify(&name);
    let slug = non_empty(overrides.slug)
        .unwrap_or(&derived_slug)
        .to_string();

    let version = non_empty(overrides.version)
        .map(str::to_string)
        .or_else(|| config.version.clone())
        .unwrap_or_else(|| "1.0.0".to_string());

    let short_description = overrides
        .short_description
        .map(str::to_string)
        .or_else(|| marketplace.short_description.clone())
        .unwrap_or_default();

    let long_description = overrides
        .long_description
        .map(str::to_string)
        .or_else(|| overrides.long_description_file.map(str::to_string))
        .or_else(|| marketplace.long_description.clone())
        .unwrap_or_default();

    let violations = crate::models::listing_description_violations(
        Some(&short_description),
        Some(&long_description),
    );
    if !violations.is_empty() {
        let messages: Vec<String> = violations
            .iter()
            .map(|violation| format!("{} {}", violation.message, listing_hint(violation.field)))
            .collect();
        return Err(messages.join("\n"));
    }

    let category = non_empty(overrides.category)
        .map(str::to_string)
        .or_else(|| non_empty(marketplace.category.as_deref()).map(str::to_string));

    let plan_type = match non_empty(overrides.plan_type) {
        Some(plan_type) => plan_type.to_string(),
        None => marketplace
            .resolved_plan_type()?
            .unwrap_or_else(|| "free".to_string()),
    };

    let price = overrides.price.or_else(|| {
        marketplace
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.price)
    });

    let currency = marketplace
        .pricing
        .as_ref()
        .and_then(|pricing| non_empty(pricing.currency.as_deref()).map(str::to_string));

    let tags = marketplace
        .tags
        .iter()
        .filter_map(|tag| non_empty(Some(tag.as_str())).map(str::to_string))
        .collect();

    let vendor_url = non_empty(marketplace.support_url.as_deref()).map(str::to_string);

    let confirm_no_secrets = marketplace.no_secrets_confirmation.unwrap_or(true);

    Ok(ListingMetadata {
        name,
        slug,
        version,
        short_description,
        long_description,
        category,
        plan_type,
        price,
        currency,
        tags,
        vendor_url,
        confirm_no_secrets,
    })
}

fn build_template_body(
    listing: &ListingMetadata,
    stack_definition: serde_json::Value,
    config_contract: Option<serde_json::Value>,
    source_project_id: Option<i32>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "name": listing.name,
        "slug": listing.slug,
        "version": listing.version,
        "short_description": listing.short_description,
        "long_description": listing.long_description,
        "stack_definition": stack_definition,
        "plan_type": listing.plan_type,
    });

    if let Some(category) = &listing.category {
        body["category_code"] = serde_json::json!(category);
    }
    if let Some(price) = listing.price {
        body["price"] = serde_json::json!(price);
    }
    if let Some(currency) = &listing.currency {
        body["currency"] = serde_json::json!(currency);
    }
    if !listing.tags.is_empty() {
        body["tags"] = serde_json::json!(listing.tags);
    }
    if let Some(vendor_url) = &listing.vendor_url {
        body["vendor_url"] = serde_json::json!(vendor_url);
    }
    if let Some(config_contract) = config_contract {
        body["config_contract"] = config_contract;
    }
    if let Some(source_project_id) = source_project_id {
        body["source_project_id"] = serde_json::json!(source_project_id);
    }

    body
}

impl CallableTrait for SubmitCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 1. Load credentials
        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("submit")?;
        let base_url = crate::cli::install_runner::normalize_stacker_server_url(
            creds
                .server_url
                .as_deref()
                .unwrap_or(crate::cli::stacker_client::DEFAULT_STACKER_URL),
        );

        // 2. Read and parse stacker.yml
        let project_dir = std::env::current_dir()?;
        let config_path = match &self.file {
            Some(f) => Path::new(f).to_path_buf(),
            None => project_dir.join(DEFAULT_CONFIG_FILE),
        };

        let config = StackerConfig::from_file(&config_path)?;
        let source_project_id = DeploymentLock::load_active(&project_dir)?
            .and_then(|lock| lock.project_id)
            .and_then(|id| i32::try_from(id).ok());

        // Read the raw YAML content to send as the stack definition
        let raw_yaml = std::fs::read_to_string(&config_path)?;
        let stack_definition: serde_json::Value =
            serde_json::to_value(&serde_yaml::from_str::<serde_yaml::Value>(&raw_yaml)?)?;

        let long_description_file = match &self.long_description_file {
            Some(path) => {
                let path = PathBuf::from(path);
                Some(std::fs::read_to_string(&path).map_err(|err| {
                    CliError::ConfigValidation(format!(
                        "Failed to read --long-description-file '{}': {}",
                        path.display(),
                        err
                    ))
                })?)
            }
            None => None,
        };

        let overrides = SubmitOverrides {
            slug: self.slug.as_deref(),
            version: self.version.as_deref(),
            short_description: self.description.as_deref(),
            long_description: self.long_description.as_deref(),
            long_description_file: long_description_file.as_deref(),
            category: self.category.as_deref(),
            plan_type: self.plan_type.as_deref(),
            price: self.price,
        };
        let listing = resolve_listing(&config, &overrides).map_err(CliError::ConfigValidation)?;

        let config_contract = serde_json::to_value(&config.config_contract)?;
        let config_contract = if config_contract
            .get("services")
            .and_then(|services| services.as_object())
            .is_some_and(|services| !services.is_empty())
        {
            Some(config_contract)
        } else {
            None
        };

        let body = build_template_body(
            &listing,
            stack_definition,
            config_contract,
            source_project_id,
        );

        // 3. Create async runtime and execute
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                CliError::ConfigValidation(format!("Failed to create async runtime: {}", e))
            })?;

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);

            // Create or update the template on the server
            eprintln!("Creating/updating template '{}'...", listing.name);
            let template = client.marketplace_create_or_update(body.clone()).await?;

            // Submit for review — use resubmit endpoint for templates that
            // are already approved (submit_for_review only allows
            // draft/rejected/needs_changes).
            if template.status == "approved" {
                eprintln!("Resubmitting approved template for marketplace review...");
                let mut resubmit_body = body.clone();
                resubmit_body["confirm_no_secrets"] = serde_json::json!(listing.confirm_no_secrets);
                client
                    .marketplace_resubmit(&template.id, resubmit_body)
                    .await?;
            } else {
                eprintln!("Submitting for marketplace review...");
                client.marketplace_submit(&template.id).await?;
            }

            // Success message
            println!();
            println!(
                "Submitted '{}' v{} for marketplace review.",
                listing.name, listing.version
            );
            println!(
                "Your stack will be published automatically once accepted by the review team."
            );
            println!(
                "Check status with: stacker marketplace status {}",
                listing.name
            );

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG_DESCRIPTION: &str = "Deploy a complete self-hosted stack: the app, its \
database and background workers come pre-wired and ready for production use on a fresh \
server after a one-click install.";

    const FLAG_LONG_DESCRIPTION: &str = "From the --long-description flag: a full markdown \
listing body covering use cases, requirements and customisation, long enough to pass review.";

    const FILE_LONG_DESCRIPTION: &str = "Long description loaded from a file on disk, padded \
to the length the marketplace review checklist requires for the listing page.";

    const FLAG_BEATS_FILE_DESCRIPTION: &str = "Flag beats the file source and is long enough \
to satisfy the marketplace listing minimum of one hundred characters.";

    fn config_with(yaml: &str) -> StackerConfig {
        StackerConfig::from_str(yaml).expect("test config should parse")
    }

    fn no_overrides() -> SubmitOverrides<'static> {
        SubmitOverrides {
            slug: None,
            version: None,
            short_description: None,
            long_description: None,
            long_description_file: None,
            category: None,
            plan_type: None,
            price: None,
        }
    }

    #[test]
    fn resolve_listing_reads_marketplace_block() {
        let config = config_with(&format!(
            r#"
name: ai-helpdesk-starter
marketplace:
  publish: true
  display_name: "Internal AI Helpdesk"
  short_description: "Self-hosted AI helpdesk with n8n workflows and Ollama."
  long_description: '{LONG_DESCRIPTION}'
  category: ai-agents
  tags: [n8n, ollama]
  license: paid
  pricing:
    price: 49
    currency: USD
  support_url: https://github.com/your-org/ai-helpdesk-starter
  no_secrets_confirmation: true
"#
        ));

        let listing = resolve_listing(&config, &no_overrides()).expect("listing should resolve");
        assert_eq!("Internal AI Helpdesk", listing.name);
        assert_eq!("internal-ai-helpdesk", listing.slug);
        assert_eq!("1.0.0", listing.version);
        assert_eq!(
            "Self-hosted AI helpdesk with n8n workflows and Ollama.",
            listing.short_description
        );
        assert_eq!(LONG_DESCRIPTION, listing.long_description);
        assert_eq!(Some("ai-agents".to_string()), listing.category);
        assert_eq!("one_time", listing.plan_type);
        assert_eq!(Some(49.0), listing.price);
        assert_eq!(Some("USD".to_string()), listing.currency);
        assert_eq!(vec!["n8n".to_string(), "ollama".to_string()], listing.tags);
        assert_eq!(
            Some("https://github.com/your-org/ai-helpdesk-starter".to_string()),
            listing.vendor_url
        );
        assert!(listing.confirm_no_secrets);
    }

    #[test]
    fn resolve_listing_flags_override_marketplace_block() {
        let config = config_with(&format!(
            r#"
name: ai-helpdesk-starter
version: "1.0.0"
marketplace:
  short_description: "From stacker.yml short description."
  long_description: '{LONG_DESCRIPTION}'
  category: from-yaml
  license: subscription
"#
        ));

        let overrides = SubmitOverrides {
            slug: Some("custom-slug"),
            version: Some("2.0.0"),
            short_description: Some("From the --description flag."),
            long_description: Some(FLAG_LONG_DESCRIPTION),
            category: Some("from-flag"),
            plan_type: Some("free"),
            price: Some(9.0),
            ..no_overrides()
        };
        let listing = resolve_listing(&config, &overrides).expect("listing should resolve");
        assert_eq!("custom-slug", listing.slug);
        assert_eq!("2.0.0", listing.version);
        assert_eq!("From the --description flag.", listing.short_description);
        assert_eq!(FLAG_LONG_DESCRIPTION, listing.long_description);
        assert_eq!(Some("from-flag".to_string()), listing.category);
        assert_eq!("free", listing.plan_type);
        assert_eq!(Some(9.0), listing.price);
    }

    #[test]
    fn resolve_listing_long_description_sources_precedence() {
        let config = config_with(&format!(
            r#"
name: my-stack
marketplace:
  short_description: "Short description from the stacker.yml block."
  long_description: '{LONG_DESCRIPTION}'
"#
        ));

        let from_file = SubmitOverrides {
            long_description_file: Some(FILE_LONG_DESCRIPTION),
            ..no_overrides()
        };
        let listing = resolve_listing(&config, &from_file).expect("listing should resolve");
        assert_eq!(FILE_LONG_DESCRIPTION, listing.long_description);

        let both = SubmitOverrides {
            long_description: Some(FLAG_BEATS_FILE_DESCRIPTION),
            long_description_file: Some("File source loses to the flag."),
            ..no_overrides()
        };
        let listing = resolve_listing(&config, &both).expect("listing should resolve");
        assert_eq!(FLAG_BEATS_FILE_DESCRIPTION, listing.long_description);

        let listing = resolve_listing(&config, &no_overrides()).expect("listing should resolve");
        assert_eq!(LONG_DESCRIPTION, listing.long_description);
    }

    #[test]
    fn resolve_listing_rejects_missing_descriptions_with_hints() {
        let config = config_with("name: my-stack\n");
        let err = resolve_listing(&config, &no_overrides()).unwrap_err();
        assert!(err.contains("short_description"), "{err}");
        assert!(err.contains("--description"), "{err}");
        assert!(err.contains("long_description"), "{err}");
        assert!(err.contains("--long-description"), "{err}");
    }

    #[test]
    fn resolve_listing_rejects_too_short_descriptions() {
        let config = config_with(
            r#"
name: my-stack
marketplace:
  short_description: "Too short"
  long_description: "Missing long description"
"#,
        );
        let err = resolve_listing(&config, &no_overrides()).unwrap_err();
        assert!(err.contains("too short"), "{err}");
    }

    #[test]
    fn resolve_listing_refuses_opt_out_flags() {
        let publish_false = config_with(
            r#"
name: my-stack
marketplace:
  publish: false
"#,
        );
        let err = resolve_listing(&publish_false, &no_overrides()).unwrap_err();
        assert!(err.contains("marketplace.publish"), "{err}");

        let secrets_false = config_with(&format!(
            r#"
name: my-stack
marketplace:
  short_description: "A short description of the stack."
  long_description: '{LONG_DESCRIPTION}'
  no_secrets_confirmation: false
"#
        ));
        let err = resolve_listing(&secrets_false, &no_overrides()).unwrap_err();
        assert!(err.contains("no_secrets_confirmation"), "{err}");
    }

    #[test]
    fn build_template_body_includes_listing_metadata() {
        let listing = ListingMetadata {
            name: "Internal AI Helpdesk".to_string(),
            slug: "internal-ai-helpdesk".to_string(),
            version: "1.2.0".to_string(),
            short_description: "Self-hosted AI helpdesk with n8n workflows.".to_string(),
            long_description: LONG_DESCRIPTION.to_string(),
            category: Some("ai-agents".to_string()),
            plan_type: "one_time".to_string(),
            price: Some(49.0),
            currency: Some("USD".to_string()),
            tags: vec!["n8n".to_string()],
            vendor_url: Some("https://example.com/support".to_string()),
            confirm_no_secrets: true,
        };

        let body = build_template_body(
            &listing,
            serde_json::json!({ "services": {} }),
            Some(serde_json::json!({ "services": {} })),
            Some(42),
        );

        assert_eq!("Internal AI Helpdesk", body["name"]);
        assert_eq!("internal-ai-helpdesk", body["slug"]);
        assert_eq!("1.2.0", body["version"]);
        assert_eq!(
            "Self-hosted AI helpdesk with n8n workflows.",
            body["short_description"]
        );
        assert_eq!(LONG_DESCRIPTION, body["long_description"]);
        assert_eq!("ai-agents", body["category_code"]);
        assert_eq!("one_time", body["plan_type"]);
        assert_eq!(49.0, body["price"]);
        assert_eq!("USD", body["currency"]);
        assert_eq!(serde_json::json!(["n8n"]), body["tags"]);
        assert_eq!("https://example.com/support", body["vendor_url"]);
        assert_eq!(42, body["source_project_id"]);
    }

    #[test]
    fn build_template_body_omits_empty_optional_fields() {
        let listing = ListingMetadata {
            name: "Minimal Stack".to_string(),
            slug: "minimal-stack".to_string(),
            version: "1.0.0".to_string(),
            short_description: "A minimal stack with a short description.".to_string(),
            long_description: LONG_DESCRIPTION.to_string(),
            category: None,
            plan_type: "free".to_string(),
            price: None,
            currency: None,
            tags: Vec::new(),
            vendor_url: None,
            confirm_no_secrets: true,
        };

        let body = build_template_body(&listing, serde_json::json!({}), None, None);

        assert_eq!("free", body["plan_type"]);
        assert!(body.get("category_code").is_none());
        assert!(body.get("price").is_none());
        assert!(body.get("tags").is_none());
        assert!(body.get("vendor_url").is_none());
        assert!(body.get("config_contract").is_none());
        assert!(body.get("source_project_id").is_none());
    }
}
