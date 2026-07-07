use crate::cli::config_parser::{StackerConfig, MARKETPLACE_ORIGIN_MARKER};
use crate::cli::credentials::CredentialsManager;
use crate::cli::deployment_lock::DeploymentLock;
use crate::cli::error::CliError;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::{build_deploy_form, MarketplaceTemplate, StackerClient};
use crate::console::commands::CallableTrait;
use dialoguer::Confirm;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// marketplace templates (list all / browse catalog)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct MarketplaceTemplatesCommand {
    category: Option<String>,
    tag: Option<String>,
    json: bool,
}

impl MarketplaceTemplatesCommand {
    pub fn new(category: Option<String>, tag: Option<String>, json: bool) -> Self {
        Self { category, tag, json }
    }
}

impl CallableTrait for MarketplaceTemplatesCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("list marketplace templates")?;
        let templates = ctx.block_on(async {
            ctx.client
                .list_marketplace_templates(self.category.as_deref(), self.tag.as_deref())
                .await
        })?;

        if self.json {
            println!("{}", serde_json::to_string_pretty(&templates)?);
            return Ok(());
        }

        if templates.is_empty() {
            let filter_hint = match (&self.category, &self.tag) {
                (Some(cat), None) => format!(" for category '{}'", cat),
                (None, Some(tag)) => format!(" for tag '{}'", tag),
                (Some(cat), Some(tag)) => format!(" for category '{}' and tag '{}'", cat, tag),
                (None, None) => String::new(),
            };
            println!("No marketplace templates found{}.", filter_hint);
            println!("Browse the catalog at: https://try.direct/applications");
            return Ok(());
        }

        println!(
            "{:<24} {:<28} {:<14} {}",
            "ITEM", "NAME", "PLAN", "DESCRIPTION"
        );
        println!("{}", "\u{2500}".repeat(92));
        for template in &templates {
            println!(
                "{:<24} {:<28} {:<14} {}",
                truncate(&template.slug, 22),
                truncate(&template.name, 26),
                display_plan(template),
                truncate(template.description.as_deref().unwrap_or(""), 26),
            );
        }
        eprintln!(
            "\nInstall one with: stacker install {}",
            templates
                .first()
                .map(|template| template.slug.as_str())
                .unwrap_or("<item>")
        );

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// marketplace find/install
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct MarketplaceFindCommand {
    query: String,
    json: bool,
    limit: Option<u32>,
}

impl MarketplaceFindCommand {
    pub fn new(query: String, json: bool, limit: Option<u32>) -> Self {
        Self { query, json, limit }
    }
}

impl CallableTrait for MarketplaceFindCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("find marketplace templates")?;

        let is_list_all = self.query.eq_ignore_ascii_case("all") || self.query == "*";

        let templates = ctx.block_on(async {
            if is_list_all {
                ctx.client.list_marketplace_templates(None, None).await
            } else {
                ctx.client
                    .search_marketplace_templates(
                        Some(&self.query),
                        None,
                        None,
                        Some(self.limit.unwrap_or(20)),
                    )
                    .await
            }
        })?;

        if self.json {
            println!("{}", serde_json::to_string_pretty(&templates)?);
            return Ok(());
        }

        if templates.is_empty() {
            if is_list_all {
                println!("No marketplace templates found.");
                println!("Browse the catalog at: https://try.direct/applications");
            } else {
                println!(
                    "No marketplace templates found for '{}'. Try 'stacker find all' to browse everything.",
                    self.query
                );
            }
            return Ok(());
        }

        println!(
            "{:<24} {:<28} {:<14} {}",
            "ITEM", "NAME", "PLAN", "DESCRIPTION"
        );
        println!("{}", "\u{2500}".repeat(92));
        for template in &templates {
            println!(
                "{:<24} {:<28} {:<14} {}",
                truncate(&template.slug, 22),
                truncate(&template.name, 26),
                display_plan(template),
                truncate(template.description.as_deref().unwrap_or(""), 26),
            );
        }
        eprintln!(
            "\nInstall one with: stacker install {}",
            templates
                .first()
                .map(|template| template.slug.as_str())
                .unwrap_or("<item>")
        );

        Ok(())
    }
}

pub struct MarketplaceInstallCommand {
    template: String,
    name: Option<String>,
    file: PathBuf,
    force: bool,
    json: bool,
    domain: Option<String>,
    set_values: Vec<String>,
}

impl MarketplaceInstallCommand {
    pub fn new(
        template: String,
        name: Option<String>,
        file: Option<PathBuf>,
        force: bool,
        json: bool,
        domain: Option<String>,
        set_values: Vec<String>,
    ) -> Self {
        Self {
            template,
            name,
            file: file.unwrap_or_else(|| PathBuf::from("stacker.yml")),
            force,
            json,
            domain,
            set_values,
        }
    }
}

impl CallableTrait for MarketplaceInstallCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Refuse to run inside an existing project directory.
        // 'stacker install' creates a whole new project — it does not extend
        // the current one. Users should use 'stacker service add' instead.
        if self.file.exists() {
            let project_dir = self
                .file
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            if DeploymentLock::exists(project_dir) {
                return Err(Box::new(CliError::ConfigValidation(
                     "This directory already contains a deployed project.\n\
                     'stacker install' creates a new project from a template — it cannot \
                     be used inside an existing project directory.\n\n\
                     To extend your current project, use:\n  \
                     stacker service add <service>\n  \
                     stacker agent deploy-app <stack_code>\n\n\
                     To create a new project, run 'stacker install' in an empty directory."
                        .to_string(),
                )));
            }
        }

        let ctx = CliRuntime::new("install marketplace template")?;

        let (mut deploy_form, config_inputs, generated_stacker_yml) = if self.file.exists() {
            let config = StackerConfig::from_file(&self.file)?;
            let mut form = build_deploy_form(&config);
            if let Some(stack) = form
                .get_mut("stack")
                .and_then(|value| value.as_object_mut())
            {
                stack.insert(
                    "stack_code".to_string(),
                    serde_json::Value::String(self.template.clone()),
                );
            }
            (Some(form), config.install.inputs, false)
        } else {
            // No local stacker.yml. If this is a catalog application, we cannot
            // install it without deploy context — instead of erroring out and
            // leaving the user stuck, ask permission to generate a minimal
            // stacker.yml using --domain and proceed with a real deployment.
            let needs_deploy_context = ctx.block_on(async {
                catalog_app_needs_deploy_context(&ctx.client, &self.template).await
            })?;

            if needs_deploy_context {
                if !self.force && !confirm_generate_stacker_file(&self.file, &self.template)? {
                    println!(
                        "Aborted: no {} written and no remote project was created.",
                        self.file.display()
                    );
                    return Ok(());
                }
                let config = ctx.block_on(async {
                    generate_minimal_install_config(
                        &ctx.client,
                        &self.template,
                        self.name.as_deref(),
                        self.domain.as_deref(),
                    )
                    .await
                })?;
                let yaml = serde_yaml::to_string(&config).map_err(|err| {
                    CliError::ConfigValidation(format!("Failed to render stacker.yml: {}", err))
                })?;
                std::fs::write(&self.file, prepend_marketplace_marker(&yaml))?;
                let mut form = build_deploy_form(&config);
                if let Some(stack) = form
                    .get_mut("stack")
                    .and_then(|value| value.as_object_mut())
                {
                    stack.insert(
                        "stack_code".to_string(),
                        serde_json::Value::String(self.template.clone()),
                    );
                }
                if let Some(cloud) = config.deploy.cloud.as_ref() {
                    if let Some(key) = cloud.key.as_deref() {
                        if let Some(form_cloud) = form
                            .get_mut("cloud")
                            .and_then(|value| value.as_object_mut())
                        {
                            form_cloud.insert(
                                "name".to_string(),
                                serde_json::Value::String(key.to_string()),
                            );
                        }
                    }
                }
                (Some(form), config.install.inputs, true)
            } else {
                (None, Default::default(), false)
            }
        };

        // If the deploy form has no cloud credential name, try to resolve
        // one from the project's existing cloud deployment lock file.
        if let Some(form) = deploy_form.as_mut() {
            let has_cloud_name = form
                .get("cloud")
                .and_then(|c| c.get("name"))
                .and_then(|n| n.as_str())
                .filter(|s| !s.is_empty())
                .is_some();

            if !has_cloud_name {
                let project_dir = self
                    .file
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));

                if let Ok(Some(lock)) = DeploymentLock::load_for_target(project_dir, "cloud") {
                    if let Some(cloud_id) = lock.cloud_id {
                        let cloud_info = ctx
                            .block_on(async { ctx.client.get_cloud(cloud_id).await })
                            .ok()
                            .flatten();

                        if let Some(cloud) = cloud_info {
                            if let Some(form_cloud) =
                                form.get_mut("cloud").and_then(|v| v.as_object_mut())
                            {
                                form_cloud.insert(
                                    "name".to_string(),
                                    serde_json::Value::String(cloud.name.clone()),
                                );
                                form_cloud.insert(
                                    "save_token".to_string(),
                                    serde_json::Value::Bool(false),
                                );
                                eprintln!(
                                    "Using cloud credential '{}' from previous deployment.",
                                    cloud.name
                                );
                            }
                        }
                    }
                }
            }
        }

        let install_inputs =
            resolve_install_inputs(config_inputs, self.domain.as_deref(), &self.set_values)?;
        if let Some(form) = deploy_form.as_mut() {
            apply_install_inputs_to_deploy_form(form, &install_inputs);
        }
        let install_inputs = if install_inputs.is_empty() {
            None
        } else {
            Some(install_inputs)
        };

        if generated_stacker_yml {
            println!(
                "Wrote {} with defaults. Starting deployment...",
                self.file.display()
            );
        }

        let response = ctx.block_on(async {
            ctx.client
                .install_marketplace_template(
                    &self.template,
                    self.name.as_deref(),
                    deploy_form,
                    install_inputs,
                )
                .await
        })?;

        if self.json {
            println!("{}", serde_json::to_string_pretty(&response)?);
            return Ok(());
        }

        if let Some(deployment_id) = response.deployment_id {
            println!(
                "Installed '{}' as project #{} and started deployment #{}.",
                response.template.slug, response.project.id, deployment_id
            );
            println!("Track with: stacker deployments state {}", deployment_id);
            return Ok(());
        }

        let Some(yaml) = response_stack_definition(&response) else {
            println!(
                "Installed '{}' as project #{}.",
                response.template.slug, response.project.id
            );
            if !generated_stacker_yml {
                println!(
                    "No local {} was written because this catalog application did not return a stack definition.",
                    self.file.display()
                );
            }
            return Ok(());
        };

        if !self.force && !confirm_stacker_file_write(&self.file)? {
            println!(
                "Installed '{}' as project #{}.",
                response.template.slug, response.project.id
            );
            println!("Skipped writing {}.", self.file.display());
            return Ok(());
        }

        std::fs::write(&self.file, prepend_marketplace_marker(&yaml))?;

        println!(
            "Installed '{}' as project #{}.",
            response.template.slug, response.project.id
        );
        println!("Wrote {}", self.file.display());
        println!(
            "  Hooks in this file will be REFUSED by `stacker deploy` until you review \
             them and remove the '{}' marker line (or pass --allow-untrusted-hooks).",
            MARKETPLACE_ORIGIN_MARKER
        );
        println!(
            "Deploy with: stacker deploy --project {}",
            response.project.name
        );

        Ok(())
    }
}

/// Prepend the marketplace-origin marker to a YAML body so that a
/// subsequent `stacker deploy` treats hook execution as untrusted.
///
/// The marker is idempotent — if the body already starts with the
/// marker line, we don't add a second copy.
fn prepend_marketplace_marker(yaml: &str) -> String {
    let already_marked = yaml
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            line.trim()
                .strip_prefix('#')
                .map(|rest| rest.trim().eq_ignore_ascii_case(
                    MARKETPLACE_ORIGIN_MARKER.trim_start_matches('#').trim(),
                ))
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if already_marked {
        yaml.to_string()
    } else {
        format!(
            "{marker}\n# Delete the line above once you have reviewed hooks in this file — \
             `stacker deploy` refuses to run hooks while the marker is present.\n{body}",
            marker = MARKETPLACE_ORIGIN_MARKER,
            body = yaml
        )
    }
}

fn response_stack_definition(
    response: &crate::cli::stacker_client::MarketplaceInstallResponse,
) -> Option<String> {
    let value = response.latest_version.get("stack_definition")?;

    if value.is_object() {
        return serde_yaml::to_string(value).ok();
    }

    if let Some(yaml_str) = value.as_str() {
        use docker_compose_types as dctypes;

        // Parse as Docker Compose and convert to stacker config format
        let compose: dctypes::Compose = serde_yaml::from_str(yaml_str).ok()?;

        let project_name = response.template.slug.clone();
        let mut config = serde_yaml::Mapping::new();
        config.insert("name".into(), serde_yaml::Value::String(project_name));

        let mut svc_map = serde_yaml::Mapping::new();
        for (svc_name, svc_opt) in &compose.services.0 {
            let Some(service) = svc_opt.as_ref() else { continue };
            let mut out = serde_yaml::Mapping::new();

            // image
            if let Some(image) = &service.image {
                out.insert("image".into(), serde_yaml::Value::String(image.clone()));
            }

            // ports: follow the same conversion pattern as builder.rs
            let ports: Vec<String> = match &service.ports {
                dctypes::Ports::Short(list) => list.clone(),
                dctypes::Ports::Long(list) => list
                    .iter()
                    .map(|p| {
                        let host = p
                            .host_ip
                            .as_ref()
                            .map(|h| format!("{}:", h))
                            .unwrap_or_default();
                        let published = p
                            .published
                            .as_ref()
                            .map(|pp| match pp {
                                dctypes::PublishedPort::Single(n) => n.to_string(),
                                dctypes::PublishedPort::Range(s) => s.clone(),
                            })
                            .unwrap_or_default();
                        format!("{}{}:{}", host, published, p.target)
                    })
                    .collect(),
            };
            if !ports.is_empty() {
                let yaml_ports: Vec<serde_yaml::Value> = ports
                    .into_iter()
                    .map(serde_yaml::Value::String)
                    .collect();
                out.insert("ports".into(), serde_yaml::Value::Sequence(yaml_ports));
            }

            // volumes
            let volumes: Vec<String> = service
                .volumes
                .iter()
                .filter_map(|v| match v {
                    dctypes::Volumes::Simple(s) => Some(s.clone()),
                    dctypes::Volumes::Advanced(adv) => Some(format!(
                        "{}:{}",
                        adv.source.as_deref().unwrap_or(""),
                        &adv.target
                    )),
                })
                .collect();
            if !volumes.is_empty() {
                let yaml_vols: Vec<serde_yaml::Value> = volumes
                    .into_iter()
                    .map(serde_yaml::Value::String)
                    .collect();
                out.insert("volumes".into(), serde_yaml::Value::Sequence(yaml_vols));
            }

            // environment
            let env_map: serde_yaml::Mapping = match &service.environment {
                dctypes::Environment::List(list) => list
                    .iter()
                    .filter_map(|entry| {
                        entry.split_once('=').map(|(k, v)| {
                            (
                                serde_yaml::Value::String(k.to_string()),
                                serde_yaml::Value::String(v.to_string()),
                            )
                        })
                    })
                    .collect(),
                dctypes::Environment::KvPair(map) => map
                    .iter()
                    .map(|(k, v)| {
                        let val = v
                            .as_ref()
                            .map(|sv| match sv {
                                dctypes::SingleValue::String(s) => s.clone(),
                                dctypes::SingleValue::Bool(b) => b.to_string(),
                                dctypes::SingleValue::Unsigned(n) => n.to_string(),
                                dctypes::SingleValue::Signed(n) => n.to_string(),
                                dctypes::SingleValue::Float(f) => f.to_string(),
                            })
                            .unwrap_or_default();
                        (
                            serde_yaml::Value::String(k.clone()),
                            serde_yaml::Value::String(val),
                        )
                    })
                    .collect(),
            };
            if !env_map.is_empty() {
                out.insert(
                    "environment".into(),
                    serde_yaml::Value::Mapping(env_map),
                );
            }

            // depends_on: simple list of strings
            let depends: Vec<String> = match &service.depends_on {
                dctypes::DependsOnOptions::Simple(list) => list.clone(),
                dctypes::DependsOnOptions::Conditional(map) => {
                    map.keys().cloned().collect()
                }
            };
            if !depends.is_empty() {
                let yaml_dep: Vec<serde_yaml::Value> = depends
                    .into_iter()
                    .map(serde_yaml::Value::String)
                    .collect();
                out.insert(
                    "depends_on".into(),
                    serde_yaml::Value::Sequence(yaml_dep),
                );
            }

            svc_map.insert(
                serde_yaml::Value::String(svc_name.clone()),
                serde_yaml::Value::Mapping(out),
            );
        }
        if !svc_map.is_empty() {
            config.insert("services".into(), serde_yaml::Value::Mapping(svc_map));
        }

        return serde_yaml::to_string(&config).ok();
    }

    None
}

fn confirm_stacker_file_write(file: &std::path::Path) -> Result<bool, CliError> {
    let action = if file.exists() { "update" } else { "create" };
    Confirm::new()
        .with_prompt(format!(
            "Stacker wants to {} {} in the current directory. Allow?",
            action,
            file.display()
        ))
        .default(false)
        .interact()
        .map_err(|err| CliError::ConfigValidation(format!("Failed to read confirmation: {}", err)))
}

async fn catalog_app_needs_deploy_context(
    client: &StackerClient,
    template: &str,
) -> Result<bool, CliError> {
    // DB-backed marketplace templates can install standalone.
    if client.get_marketplace_template(template).await?.is_some() {
        return Ok(false);
    }

    // Otherwise check whether this is a known catalog application.
    let applications = client
        .search_marketplace_templates(Some(template), None, None, Some(10))
        .await?;
    Ok(applications
        .iter()
        .any(|application| marketplace_template_matches_slug(application, template)))
}

fn confirm_generate_stacker_file(file: &Path, template: &str) -> Result<bool, CliError> {
    Confirm::new()
        .with_prompt(format!(
            "Stacker wants to create {} in the current directory with defaults to install and deploy '{}' to the cloud. Allow?",
            file.display(),
            template
        ))
        .default(true)
        .interact()
        .map_err(|err| CliError::ConfigValidation(format!("Failed to read confirmation: {}", err)))
}

fn cloud_provider_from_code(code: &str) -> Option<crate::cli::config_parser::CloudProvider> {
    use crate::cli::config_parser::CloudProvider;
    match code.to_lowercase().as_str() {
        "htz" | "hetzner" => Some(CloudProvider::Hetzner),
        "do" | "digitalocean" => Some(CloudProvider::Digitalocean),
        "aws" => Some(CloudProvider::Aws),
        "lo" | "linode" => Some(CloudProvider::Linode),
        "vu" | "vultr" => Some(CloudProvider::Vultr),
        "cnt" | "contabo" => Some(CloudProvider::Contabo),
        _ => None,
    }
}

async fn generate_minimal_install_config(
    client: &StackerClient,
    template: &str,
    name: Option<&str>,
    domain: Option<&str>,
) -> Result<StackerConfig, CliError> {
    use crate::cli::config_parser::{CloudConfig, DeployTarget};

    let mut config = StackerConfig::default();
    config.name = name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(template)
        .to_string();
    config.project.identity = Some(template.to_string());
    if let Some(domain) = domain.map(str::trim).filter(|value| !value.is_empty()) {
        config.install.inputs.insert(
            "commonDomain".to_string(),
            Value::String(domain.to_ascii_lowercase()),
        );
    }

    // Install implies remote deployment. Resolve the user's default cloud
    // credential (first one returned by /cloud); if none is configured, ask
    // the user to connect a cloud provider first instead of silently
    // falling back to local.
    let clouds = client.list_clouds().await?;
    let cloud_info = clouds.into_iter().next().ok_or_else(|| {
        CliError::ConfigValidation(
            "No cloud connections found. Connect one with `stacker config cloud add` (or `stacker login`) and retry.".to_string(),
        )
    })?;
    let provider = cloud_provider_from_code(&cloud_info.provider).ok_or_else(|| {
        CliError::ConfigValidation(format!(
            "Unsupported cloud provider '{}' on default cloud '{}'.",
            cloud_info.provider, cloud_info.name
        ))
    })?;
    eprintln!(
        "Using cloud connection '{}' (provider: {}).",
        cloud_info.name, cloud_info.provider
    );

    let cloud_config = CloudConfig {
        provider,
        orchestrator: Default::default(),
        region: None,
        size: None,
        install_image: None,
        remote_payload_file: None,
        ssh_key: None,
        key: Some(cloud_info.name.clone()),
        server: None,
        public_ports: Vec::new(),
    };

    // Auto-generated stacker.yml never reuses an existing server. A fresh
    // server is always provisioned on the resolved cloud. To reuse an
    // existing one, set `deploy.cloud.server: <name>` in stacker.yml.
    eprintln!("A fresh server will be provisioned on this cloud.");

    config.deploy.target = DeployTarget::Cloud;
    config.deploy.cloud = Some(cloud_config);
    Ok(config)
}

fn marketplace_template_matches_slug(template: &MarketplaceTemplate, slug: &str) -> bool {
    template.slug.eq_ignore_ascii_case(slug)
}

fn normalize_install_input_key(key: &str) -> String {
    match key.trim() {
        "domain" | "base_domain" => "commonDomain".to_string(),
        other => other.to_string(),
    }
}

fn parse_set_value(entry: &str) -> Result<(String, Value), CliError> {
    let (key, value) = entry.split_once('=').ok_or_else(|| {
        CliError::ConfigValidation(format!(
            "Invalid --set value '{}'. Expected KEY=VALUE.",
            entry
        ))
    })?;
    let key = normalize_install_input_key(key);
    if key.trim().is_empty() {
        return Err(CliError::ConfigValidation(
            "Invalid --set value: key cannot be empty".to_string(),
        ));
    }
    Ok((key, Value::String(value.trim().to_string())))
}

fn resolve_install_inputs(
    mut inputs: Map<String, Value>,
    domain: Option<&str>,
    set_values: &[String],
) -> Result<Map<String, Value>, CliError> {
    if let Some(value) = inputs.remove("base_domain") {
        inputs.entry("commonDomain".to_string()).or_insert(value);
    }
    if let Some(value) = inputs.remove("domain") {
        inputs.entry("commonDomain".to_string()).or_insert(value);
    }
    if let Some(domain) = domain.map(str::trim).filter(|value| !value.is_empty()) {
        inputs.insert(
            "commonDomain".to_string(),
            Value::String(domain.to_ascii_lowercase()),
        );
    }
    for entry in set_values {
        let (key, value) = parse_set_value(entry)?;
        inputs.insert(key, value);
    }
    Ok(inputs)
}

fn apply_install_inputs_to_deploy_form(form: &mut Value, inputs: &Map<String, Value>) {
    if inputs.is_empty() {
        return;
    }
    if let Some(common_domain) = inputs.get("commonDomain").cloned() {
        if let Some(obj) = form.as_object_mut() {
            obj.insert("commonDomain".to_string(), common_domain);
        }
    }
    let Some(stack) = form
        .get_mut("stack")
        .and_then(|value| value.as_object_mut())
    else {
        return;
    };
    let vars = stack
        .entry("vars".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(vars) = vars.as_array_mut() else {
        return;
    };
    for (key, value) in inputs {
        vars.retain(|entry| entry.get("key").and_then(Value::as_str) != Some(key.as_str()));
        vars.push(serde_json::json!({ "key": key, "value": value }));
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// marketplace status
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker marketplace status [name] [--json]`
///
/// Check submission status for the user's marketplace templates.
/// If a name is provided, shows detail for that template only.
pub struct MarketplaceStatusCommand {
    name: Option<String>,
    json: bool,
}

impl MarketplaceStatusCommand {
    pub fn new(name: Option<String>, json: bool) -> Self {
        Self { name, json }
    }
}

impl CallableTrait for MarketplaceStatusCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.json;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("marketplace status")?;
        let base_url = crate::cli::install_runner::normalize_stacker_server_url(
            crate::cli::stacker_client::DEFAULT_STACKER_URL,
        );

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                CliError::ConfigValidation(format!("Failed to create async runtime: {}", e))
            })?;

        let name = self.name.clone();

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);
            let templates = client.marketplace_list_mine().await?;

            if let Some(ref name) = name {
                // Filter to matching template
                let template = templates
                    .iter()
                    .find(|t| t.name == *name || t.slug == *name);
                match template {
                    Some(t) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&t)?);
                        } else {
                            println!(
                                "Stack:      {} v{}",
                                t.name,
                                t.version.as_deref().unwrap_or("?")
                            );
                            println!("Status:     {}", t.status);
                            println!(
                                "Submitted:  {}",
                                t.created_at.as_deref().unwrap_or("\u{2014}")
                            );
                            if let Some(ref reason) = t.review_reason {
                                println!("Reason:     {}", reason);
                            }
                        }
                    }
                    None => {
                        eprintln!("No submission found for '{}'", name);
                        std::process::exit(1);
                    }
                }
            } else {
                if json {
                    println!("{}", serde_json::to_string_pretty(&templates)?);
                } else {
                    if templates.is_empty() {
                        println!("No marketplace submissions found.");
                        println!("Submit your first stack with: stacker submit");
                        return Ok(());
                    }
                    println!(
                        "{:<25} {:<10} {:<15} {:<20}",
                        "STACK", "VERSION", "STATUS", "SUBMITTED"
                    );
                    println!("{}", "\u{2500}".repeat(72));
                    for t in &templates {
                        println!(
                            "{:<25} {:<10} {:<15} {:<20}",
                            truncate(&t.name, 23),
                            t.version.as_deref().unwrap_or("\u{2014}"),
                            t.status,
                            t.created_at.as_deref().unwrap_or("\u{2014}"),
                        );
                    }
                    eprintln!("\n{} submission(s) total.", templates.len());
                }
            }
            Ok(())
        })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// marketplace logs
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker marketplace logs <name> [--json]`
///
/// Show review comments and history for a marketplace submission.
pub struct MarketplaceLogsCommand {
    name: String,
    json: bool,
}

impl MarketplaceLogsCommand {
    pub fn new(name: String, json: bool) -> Self {
        Self { name, json }
    }
}

impl CallableTrait for MarketplaceLogsCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.json;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("marketplace logs")?;
        let base_url = crate::cli::install_runner::normalize_stacker_server_url(
            crate::cli::stacker_client::DEFAULT_STACKER_URL,
        );

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                CliError::ConfigValidation(format!("Failed to create async runtime: {}", e))
            })?;

        let name = self.name.clone();

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);

            // First, find the template by name to get its ID
            let templates = client.marketplace_list_mine().await?;
            let template = templates.iter().find(|t| t.name == name || t.slug == name);

            let template = match template {
                Some(t) => t,
                None => {
                    eprintln!("No submission found for '{}'", name);
                    std::process::exit(1);
                }
            };

            let reviews = client.marketplace_reviews(&template.id).await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&reviews)?);
            } else {
                println!(
                    "Review history for: {} v{}",
                    template.name,
                    template.version.as_deref().unwrap_or("?")
                );
                println!("Current status: {}", template.status);
                println!();

                if reviews.is_empty() {
                    println!("No reviews yet.");
                    return Ok(());
                }

                println!(
                    "{:<12} {:<20} {:<20} {}",
                    "DECISION", "SUBMITTED", "REVIEWED", "REASON"
                );
                println!("{}", "\u{2500}".repeat(80));
                for r in &reviews {
                    println!(
                        "{:<12} {:<20} {:<20} {}",
                        r.decision,
                        r.submitted_at.as_deref().unwrap_or("\u{2014}"),
                        r.reviewed_at.as_deref().unwrap_or("\u{2014}"),
                        r.review_reason.as_deref().unwrap_or(""),
                    );
                }
                eprintln!("\n{} review(s) total.", reviews.len());
            }
            Ok(())
        })
    }
}

// ── helpers ──────────────────────────────────────────

/// Format the plan/price column for display.
/// Shows `required_plan_name` if set, otherwise formats `price` with currency,
/// or "free" if neither indicates a paid template.
fn display_plan(template: &MarketplaceTemplate) -> String {
    if let Some(plan) = template.required_plan_name.as_deref().filter(|p| !p.is_empty()) {
        return plan.to_string();
    }
    if let Some(price) = template.price {
        if price > 0.0 {
            let cycle = template
                .billing_cycle
                .as_deref()
                .unwrap_or("/mo");
            let cycle = match cycle {
                "one_time" | "one-time" | "once" | "free" => "",
                "monthly" | "month" | "/mo" => "/mo",
                "yearly" | "year" | "/yr" => "/yr",
                other => other,
            };
            return format!("${:.2}{}", price, cycle);
        }
    }
    "free".to_string()
}

/// Truncate a string to `max_len` characters, adding "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{}\u{2026}", truncated)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_install_inputs_to_deploy_form, marketplace_template_matches_slug,
        resolve_install_inputs, response_stack_definition,
    };
    use crate::cli::stacker_client::{
        MarketplaceInstallResponse, MarketplaceTemplate, ProjectInfo,
    };
    use serde_json::{json, Map, Value};

    #[test]
    fn install_domain_populates_common_domain_and_stack_vars() {
        let inputs = resolve_install_inputs(Map::new(), Some("DIFY.COM"), &[])
            .expect("inputs should resolve");
        assert_eq!(inputs.get("commonDomain"), Some(&json!("dify.com")));

        let mut form = json!({
            "stack": {
                "stack_code": "dify",
                "vars": []
            }
        });
        apply_install_inputs_to_deploy_form(&mut form, &inputs);

        assert_eq!(form["commonDomain"], json!("dify.com"));
        assert_eq!(
            form["stack"]["vars"],
            json!([{ "key": "commonDomain", "value": "dify.com" }])
        );
    }

    #[test]
    fn install_inputs_normalize_base_domain_and_cli_overrides_win() {
        let mut config_inputs = Map::new();
        config_inputs.insert("base_domain".to_string(), json!("example.com"));
        config_inputs.insert("admin_email".to_string(), json!("admin@example.com"));

        let inputs = resolve_install_inputs(
            config_inputs,
            Some("dify.com"),
            &[
                "admin_email=owner@dify.com".to_string(),
                "public_domain=app.dify.com".to_string(),
            ],
        )
        .expect("inputs should resolve");

        assert_eq!(inputs.get("commonDomain"), Some(&json!("dify.com")));
        assert_eq!(inputs.get("admin_email"), Some(&json!("owner@dify.com")));
        assert_eq!(inputs.get("public_domain"), Some(&json!("app.dify.com")));
        assert!(!inputs.contains_key("base_domain"));
    }

    #[test]
    fn apply_install_inputs_replaces_existing_vars() {
        let mut inputs = Map::new();
        inputs.insert("commonDomain".to_string(), json!("new.example.com"));
        inputs.insert("admin_email".to_string(), json!("admin@new.example.com"));
        let mut form = json!({
            "stack": {
                "vars": [
                    { "key": "commonDomain", "value": "old.example.com" },
                    { "key": "keep", "value": "yes" }
                ]
            }
        });

        apply_install_inputs_to_deploy_form(&mut form, &inputs);

        let vars = form["stack"]["vars"]
            .as_array()
            .expect("vars should be array");
        assert_eq!(
            vars.iter()
                .filter(|entry| entry.get("key") == Some(&Value::String("commonDomain".to_string())))
                .count(),
            1
        );
        assert!(vars.contains(&json!({ "key": "keep", "value": "yes" })));
        assert!(vars.contains(&json!({ "key": "commonDomain", "value": "new.example.com" })));
        assert!(vars.contains(&json!({ "key": "admin_email", "value": "admin@new.example.com" })));
    }

    #[test]
    fn response_stack_definition_ignores_null_or_missing_definitions() {
        let mut response = marketplace_install_response(json!({ "stack_definition": null }));
        assert!(response_stack_definition(&response).is_none());

        response.latest_version = json!({});
        assert!(response_stack_definition(&response).is_none());
    }

    #[test]
    fn response_stack_definition_accepts_object_definitions() {
        let response = marketplace_install_response(json!({
            "stack_definition": {
                "project": { "name": "dify" }
            }
        }));

        let result = response_stack_definition(&response);
        assert!(result.is_some());
        let yaml = result.unwrap();
        assert!(yaml.contains("project:"));
        assert!(yaml.contains("name: dify"));
    }

    #[test]
    fn response_stack_definition_accepts_string_definitions() {
        let compose = "version: '3'\nservices:\n  app:\n    image: nginx\n";
        let response = marketplace_install_response(json!({
            "stack_definition": compose
        }));

        let result = response_stack_definition(&response);
        assert!(result.is_some());
        let yaml = result.unwrap();
        // Should be converted to stacker config format (name added from template slug)
        assert!(yaml.contains("name: dify"));
        assert!(yaml.contains("image: nginx"));
    }

    #[test]
    fn marketplace_template_slug_match_is_case_insensitive() {
        let template = marketplace_template("Dify");

        assert!(marketplace_template_matches_slug(&template, "dify"));
        assert!(!marketplace_template_matches_slug(&template, "wordpress"));
    }

    fn marketplace_install_response(latest_version: Value) -> MarketplaceInstallResponse {
        MarketplaceInstallResponse {
            project: ProjectInfo {
                id: 92,
                name: "dify".to_string(),
                user_id: "user-1".to_string(),
                metadata: json!({}),
                created_at: "2026-06-16T14:29:38Z".to_string(),
                updated_at: "2026-06-16T14:29:38Z".to_string(),
            },
            template: marketplace_template("dify"),
            latest_version,
            deployment_id: None,
        }
    }

    fn marketplace_template(slug: &str) -> MarketplaceTemplate {
        MarketplaceTemplate {
            id: None,
            slug: slug.to_string(),
            name: slug.to_string(),
            description: None,
            category_code: None,
            tags: json!(null),
            status: None,
            required_plan_name: None,
            price: None,
            billing_cycle: None,
            is_from_marketplace: None,
            stack_definition: None,
        }
    }
}
