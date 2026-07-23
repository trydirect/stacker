use std::sync::Arc;

use crate::helpers::redact::{redact_sensitive_json_values, redact_yaml_string};
use actix_web::{post, web, Responder, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use serde_valid::Validate;
use sqlx::PgPool;

use crate::configuration::Settings;
use crate::connectors::errors::ConnectorError;
use crate::connectors::install_service::InstallServiceConnector;
use crate::connectors::user_service::UserServiceConnector;
use crate::forms;
use crate::forms::project::ProjectForm;
use crate::forms::project::Var;
use crate::helpers::{JsonResponse, MqManager, VaultClient};
use crate::{db, models, project_app, services};

#[derive(Debug, Deserialize)]
pub struct InstallTemplateRequest {
    pub name: Option<String>,
    #[serde(default)]
    pub deploy: Option<forms::project::Deploy>,
    #[serde(default)]
    pub install_inputs: Map<String, Value>,
    /// Client-supplied idempotency key. Required for `per_install` templates
    /// so retries collapse to a single authorization row. Omitted for
    /// free / one_time templates. If missing on a per_install install the
    /// server generates one and echoes it back in the response.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallTemplateResponse {
    pub project: models::Project,
    pub template: serde_json::Value,
    pub latest_version: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<i32>,
    /// Present only for `per_install` installs. Echoes the authorization
    /// state so the CLI (and any HTTP-level retry) can reconcile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<AuthorizationSummary>,
    /// Present only for `per_install` installs. Set to the effective key
    /// used server-side (may be server-generated if the client omitted it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationSummary {
    pub authorization_id: String,
    pub status: String,
    pub amount_minor: i64,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

fn map_access_error(err: services::MarketplaceAccessError) -> actix_web::Error {
    match err {
        services::MarketplaceAccessError::ValidationFailed(reason) => {
            tracing::error!("Marketplace access validation failed: {}", reason);
            JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Failed to validate marketplace access")
        }
        services::MarketplaceAccessError::NoPaymentMethod { .. } => {
            JsonResponse::<serde_json::Value>::build().payment_required(err.to_string())
        }
        services::MarketplaceAccessError::MissingUserToken
        | services::MarketplaceAccessError::InsufficientFeaturePlan
        | services::MarketplaceAccessError::InsufficientTemplatePlan { .. }
        | services::MarketplaceAccessError::TemplateNotOwned => {
            JsonResponse::<serde_json::Value>::build().forbidden(err.to_string())
        }
    }
}

fn normalized_project_name(name: &str) -> String {
    models::sanitize_project_name(name)
        .chars()
        .take(50)
        .collect::<String>()
}

fn build_project_form(
    template: &models::StackTemplate,
    latest_version: &models::StackTemplateVersion,
    requested_name: Option<&str>,
) -> Result<ProjectForm> {
    build_project_form_core(template, latest_version, requested_name)
        .map_err(|msg| JsonResponse::<serde_json::Value>::build().bad_request(msg))
}

/// Dry-run of the install-time project-form build. Used by `submit_handler`
/// as a submission gate so a template that can't be installed never reaches
/// the "approved" state and the marketplace listing. Errors here are the
/// same errors the install path would raise later.
pub(crate) fn validate_installable_form(
    template: &models::StackTemplate,
    latest_version: &models::StackTemplateVersion,
) -> Result<(), String> {
    build_project_form_core(template, latest_version, None).map(|_| ())
}

/// The three shapes we currently see for a stored `stack_definition`:
///
/// * `LegacyForm` — a JSON object with the `custom` wrapper, produced by
///   older submission paths. Deserializes straight into `ProjectForm`.
/// * `ComposeYaml` — a YAML string of a docker-compose file, indicated by
///   `definition_format = "yaml"`. Embedded as `docker-compose.yml`.
/// * `StackerConfig` — the shape `stacker submit` sends: the raw
///   `stacker.yml` parsed to a JSON object (no `custom` wrapper). The CLI
///   never sets `definition_format` for this shape, so we recognize it by
///   the absence of `custom`. Serialized back to YAML and embedded as
///   `stacker.yml`.
enum DefinitionShape<'a> {
    LegacyForm(&'a serde_json::Value),
    ComposeYaml(&'a str),
    StackerConfig(String),
}

fn classify_definition(
    latest_version: &models::StackTemplateVersion,
) -> Result<DefinitionShape<'_>, String> {
    let sd = &latest_version.stack_definition;
    if latest_version.definition_format.as_deref() == Some("yaml") {
        return sd
            .as_str()
            .map(DefinitionShape::ComposeYaml)
            .ok_or_else(|| "YAML stack definition is not a string".to_string());
    }
    if let Some(obj) = sd.as_object() {
        if obj.contains_key("custom") {
            return Ok(DefinitionShape::LegacyForm(sd));
        }
        let yaml = serde_yaml::to_string(sd)
            .map_err(|err| format!("Failed to serialize stacker.yml stack definition: {}", err))?;
        return Ok(DefinitionShape::StackerConfig(yaml));
    }
    Err(format!(
        "Unsupported stack_definition type: expected string (compose YAML) or object (stacker.yml / legacy form), got {}",
        match sd {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
    ))
}

fn build_project_form_core(
    template: &models::StackTemplate,
    latest_version: &models::StackTemplateVersion,
    requested_name: Option<&str>,
) -> Result<ProjectForm, String> {
    let shape = classify_definition(latest_version)
        .map_err(|err| format!("Template '{}': {}", template.slug, err))?;

    let mut form: ProjectForm = match &shape {
        DefinitionShape::LegacyForm(sd) => {
            serde_json::from_value((*sd).clone()).map_err(|err| {
                format!(
                    "Template '{}' cannot be installed because its stack definition is invalid: {}",
                    template.slug, err
                )
            })?
        }
        DefinitionShape::ComposeYaml(yaml_str) => build_form_from_yaml_embed(
            template,
            latest_version,
            requested_name,
            yaml_str,
            "docker-compose.yml",
        )?,
        DefinitionShape::StackerConfig(yaml_str) => build_form_from_yaml_embed(
            template,
            latest_version,
            requested_name,
            yaml_str,
            "stacker.yml",
        )?,
    };

    // The YAML/stacker.yml branches already synthesize a fresh `custom`
    // wrapper from the template + version; only the legacy branch needs
    // us to backfill fields on top of the parsed form.
    if matches!(shape, DefinitionShape::LegacyForm(_)) {
        if let Some(name) = requested_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let project_name = normalized_project_name(name);
            form.custom.custom_stack_code = project_name.clone();
            if form.custom.project_name.is_none() {
                form.custom.project_name = Some(project_name);
            }
        }

        form.custom.marketplace_version = Some(latest_version.version.clone());
        form.custom.marketplace_changelog = latest_version.changelog.clone();
        form.custom.marketplace_config_files = latest_version.config_files.clone();
        form.custom.marketplace_assets = latest_version.assets.clone();
        form.custom.marketplace_seed_jobs = latest_version.seed_jobs.clone();
        form.custom.marketplace_post_deploy_hooks = latest_version.post_deploy_hooks.clone();
        form.custom.marketplace_update_mode_capabilities = latest_version
            .update_mode_capabilities
            .clone()
            .unwrap_or_default();
    }

    form.validate().map_err(|err| err.to_string())?;

    Ok(form)
}

fn build_form_from_yaml_embed(
    template: &models::StackTemplate,
    latest_version: &models::StackTemplateVersion,
    requested_name: Option<&str>,
    yaml_str: &str,
    embed_name: &str,
) -> Result<ProjectForm, String> {
    let project_name = requested_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&template.slug);
    let stack_code = normalized_project_name(project_name);

    let mut config_files: Vec<serde_json::Value> =
        serde_json::from_value(latest_version.config_files.clone()).unwrap_or_default();
    let already_embedded = config_files.iter().any(|f| {
        f.get("name")
            .and_then(|n| n.as_str())
            .map(|n| n == embed_name)
            .unwrap_or(false)
    });
    if !already_embedded {
        config_files.push(serde_json::json!({
            "name": embed_name,
            "content": yaml_str,
        }));
    }

    let form_value = serde_json::json!({
        "custom": {
            "custom_stack_code": stack_code,
            "project_name": template.name.clone(),
            "custom_stack_short_description": template.short_description,
            "custom_stack_category": template.category_code.as_ref().map(|c| vec![c.clone()]),
            "web": [],
            "service": serde_json::Value::Array(vec![]),
            "feature": serde_json::Value::Array(vec![]),
            "networks": [],
            "marketplace_config_files": config_files,
            "marketplace_version": latest_version.version,
            "marketplace_changelog": latest_version.changelog,
            "marketplace_assets": latest_version.assets,
            "marketplace_seed_jobs": latest_version.seed_jobs,
            "marketplace_post_deploy_hooks": latest_version.post_deploy_hooks,
            "marketplace_update_mode_capabilities": latest_version.update_mode_capabilities,
        }
    });

    serde_json::from_value(form_value).map_err(|err| {
        format!(
            "Template '{}' has an invalid stack definition: {}",
            template.slug, err
        )
    })
}

/// Overlay the deployable fields from an enriched catalog application onto the
/// summary object. The enriched `/applications/catalog/{code}` endpoint is
/// authoritative for these, so its values win when present and non-null.
fn merge_catalog_enrichment(application: &mut Value, enriched: &Value) {
    let (Some(target), Some(source)) = (application.as_object_mut(), enriched.as_object()) else {
        return;
    };
    for key in [
        "docker_image",
        "default_port",
        "default_ports",
        "default_env",
        "default_config_files",
        // Multi-service stacks: the enriched catalog entry carries `services[]`
        // (one per member app, each with its own image) and a `kind:"stack"`
        // marker. These MUST be overlaid too, otherwise the service-less search
        // summary reaches synthesize_catalog_compose and it wrongly takes the
        // single-app path -> "missing a dockerhub_image" for real stacks.
        "services",
        "kind",
    ] {
        if let Some(value) = source.get(key).filter(|v| !v.is_null()) {
            target.insert(key.to_string(), value.clone());
        }
    }
}

fn json_scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Synthesize a minimal single-service docker-compose from a catalog
/// application's enriched fields.
///
/// A catalog application is a single container described by `docker_image`
/// (+ `default_port`/`default_ports`/`default_env`), not a multi-service
/// stack. We render one service and embed it as `docker-compose.yml`, exactly
/// how DB-backed templates ship their compose — so the rest of the deploy path
/// (including the empty-compose guard) treats it uniformly.
///
/// Returns `None` when the application has no docker image, i.e. nothing
/// deployable; the caller turns that into a clear install error.
fn synthesize_catalog_compose(application: &Value, service_code: &str) -> Option<String> {
    let mut services = serde_yaml::Mapping::new();

    // Multi-service stack: one compose service per member app, each with its
    // own real container image. Members without an image are skipped; if that
    // leaves nothing, return None so the caller can fail loudly rather than
    // shipping an empty/bogus compose.
    if let Some(members) = application.get("services").and_then(|v| v.as_array()) {
        for member in members {
            let image = member
                .get("docker_image")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let Some(image) = image else { continue };
            let key = member
                .get("code")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(service_code);
            services.insert(
                key.into(),
                build_compose_service(
                    image,
                    member.get("default_ports"),
                    member.get("default_port"),
                    member.get("default_env"),
                ),
            );
        }
        if services.is_empty() {
            return None;
        }
        return compose_document(services);
    }

    // Single catalog app: one service from the top-level docker_image.
    let image = application
        .get("docker_image")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    services.insert(
        service_code.into(),
        build_compose_service(
            image,
            application.get("default_ports"),
            application.get("default_port"),
            application.get("default_env"),
        ),
    );
    compose_document(services)
}

/// Build a single compose service mapping (image + ports + env + restart +
/// default_network) from catalog fields.
fn build_compose_service(
    image: &str,
    default_ports: Option<&Value>,
    default_port: Option<&Value>,
    default_env: Option<&Value>,
) -> serde_yaml::Value {
    // Ports: prefer an explicit `default_ports` list, else `default_port`.
    let mut ports: Vec<serde_yaml::Value> = Vec::new();
    if let Some(arr) = default_ports.and_then(|v| v.as_array()) {
        for p in arr {
            if let Some(n) = p.as_i64() {
                ports.push(serde_yaml::Value::String(format!("{n}:{n}")));
            } else if let Some(s) = p.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                let mapping = if s.contains(':') {
                    s.to_string()
                } else {
                    format!("{s}:{s}")
                };
                ports.push(serde_yaml::Value::String(mapping));
            }
        }
    }
    if ports.is_empty() {
        if let Some(n) = default_port.and_then(|v| v.as_i64()) {
            ports.push(serde_yaml::Value::String(format!("{n}:{n}")));
        }
    }

    // Environment: accept an object ({KEY: value}) or a list of {key,value} /
    // "KEY=value" entries. Anything unrecognized is skipped rather than
    // guessed at.
    let mut env = serde_yaml::Mapping::new();
    match default_env {
        Some(Value::Object(map)) => {
            for (k, v) in map {
                if let Some(s) = json_scalar_to_string(v) {
                    env.insert(k.clone().into(), s.into());
                }
            }
        }
        Some(Value::Array(arr)) => {
            for item in arr {
                if let Some(obj) = item.as_object() {
                    if let (Some(k), Some(v)) =
                        (obj.get("key").and_then(|x| x.as_str()), obj.get("value"))
                    {
                        if let Some(s) = json_scalar_to_string(v) {
                            env.insert(k.into(), s.into());
                        }
                    }
                } else if let Some((k, v)) = item.as_str().and_then(|s| s.split_once('=')) {
                    env.insert(k.into(), v.into());
                }
            }
        }
        _ => {}
    }

    let mut service = serde_yaml::Mapping::new();
    service.insert("image".into(), image.into());
    if !ports.is_empty() {
        service.insert("ports".into(), serde_yaml::Value::Sequence(ports));
    }
    if !env.is_empty() {
        service.insert("environment".into(), serde_yaml::Value::Mapping(env));
    }
    service.insert("restart".into(), "unless-stopped".into());
    service.insert(
        "networks".into(),
        serde_yaml::Value::Sequence(vec!["default_network".into()]),
    );
    serde_yaml::Value::Mapping(service)
}

/// Wrap synthesized services into a full compose document (with the shared
/// external `default_network`) and serialize to YAML.
fn compose_document(services: serde_yaml::Mapping) -> Option<String> {
    let mut default_network = serde_yaml::Mapping::new();
    default_network.insert("external".into(), serde_yaml::Value::Bool(true));
    default_network.insert("name".into(), "default_network".into());
    let mut networks = serde_yaml::Mapping::new();
    networks.insert(
        "default_network".into(),
        serde_yaml::Value::Mapping(default_network),
    );

    let mut root = serde_yaml::Mapping::new();
    root.insert("services".into(), serde_yaml::Value::Mapping(services));
    root.insert("networks".into(), serde_yaml::Value::Mapping(networks));

    serde_yaml::to_string(&serde_yaml::Value::Mapping(root)).ok()
}

fn catalog_application_project_form(
    application: &serde_json::Value,
    slug: &str,
    requested_name: Option<&str>,
) -> Result<ProjectForm> {
    let app_name = application
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or(slug);
    let project_name = requested_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(app_name);
    let stack_code = normalized_project_name(project_name);

    // Catalog apps carry no stack_definition; synthesize a compose from the
    // application's real container image(s) -- one service for a single app, or
    // one per member for a stack. Embedded as `marketplace_config_files` (same
    // channel DB templates use), it is picked up by `embedded_marketplace_compose`
    // at deploy time.
    //
    // Fail loudly if nothing could be synthesized: a missing/empty image means
    // the catalog entry is incomplete, and shipping an empty compose would only
    // surface later as an opaque `docker compose up` failure on the server.
    let marketplace_config_files = match synthesize_catalog_compose(application, &stack_code) {
        Some(compose) => serde_json::json!([{
            "name": "docker-compose.yml",
            "content": compose,
        }]),
        None => {
            let is_stack = application.get("kind").and_then(|v| v.as_str()) == Some("stack")
                || application
                    .get("services")
                    .and_then(|v| v.as_array())
                    .is_some();
            let reason = if is_stack {
                "none of its member apps resolved a container image (each member app needs its own dockerhub_image)"
            } else {
                "the catalog entry is missing a dockerhub_image"
            };
            return Err(JsonResponse::<serde_json::Value>::build().bad_request(format!(
                "Catalog application '{}' cannot be installed: no container image was available to synthesize a docker-compose file — {}.",
                slug, reason
            )));
        }
    };

    let form_value = serde_json::json!({
        "custom": {
            "custom_stack_code": stack_code,
            "project_name": project_name,
            "custom_stack_short_description": application
                .get("description")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
            "custom_stack_category": application
                .get("categories")
                .cloned()
                .or_else(|| application.get("category").map(|category| serde_json::json!([category])))
                .unwrap_or_else(|| serde_json::json!([])),
            "web": [],
            "feature": [],
            "service": [],
            "networks": [],
            "marketplace_config_files": marketplace_config_files,
            "catalog_application": application
        }
    });

    let form: ProjectForm = serde_json::from_value(form_value).map_err(|err| {
        JsonResponse::<serde_json::Value>::build().bad_request(format!(
            "Catalog application '{}' cannot be installed because its generated project form is invalid: {}",
            slug, err
        ))
    })?;

    form.validate()
        .map_err(|err| JsonResponse::<serde_json::Value>::build().bad_request(err.to_string()))?;

    Ok(form)
}

async fn insert_project_from_form(
    pg_pool: &PgPool,
    user: &models::User,
    form: &ProjectForm,
    request_json: serde_json::Value,
) -> Result<models::Project> {
    let project_name = form.custom.custom_stack_code.clone();
    let metadata = request_json.clone();

    let project = models::Project::new(user.id.clone(), project_name, metadata, request_json);

    let project = db::project::insert(pg_pool, project).await.map_err(|err| {
        tracing::error!("Failed to create installed project: {}", err);
        JsonResponse::<serde_json::Value>::build().internal_server_error("Internal Server Error")
    })?;

    project_app::sync_project_level_apps_from_form(pg_pool, project.id, form)
        .await
        .map_err(|err| {
            tracing::error!(
                "Failed to sync project-level apps for installed project {}: {}",
                project.id,
                err
            );
            JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Internal Server Error")
        })?;

    Ok(project)
}

async fn maybe_deploy_installed_project(
    user: &models::User,
    project: models::Project,
    mut deploy: Option<forms::project::Deploy>,
    slug: &str,
    pg_pool: &PgPool,
    mq_manager: &MqManager,
    settings: &Settings,
    user_service: &Arc<dyn UserServiceConnector>,
    install_service: &Arc<dyn InstallServiceConnector>,
    vault_client: &VaultClient,
    install_inputs: &Map<String, Value>,
) -> Result<(models::Project, Option<i32>)> {
    let Some(mut deploy_form) = deploy.take() else {
        return Ok((project, None));
    };

    deploy_form.stack.stack_code = Some(slug.to_string());
    apply_install_inputs_to_deploy(&mut deploy_form, install_inputs);
    let project_for_response = project.clone();
    let (_, deployment_id) = crate::routes::project::deploy::deploy_project(
        user,
        project,
        deploy_form,
        pg_pool,
        mq_manager,
        settings,
        user_service,
        install_service,
        vault_client,
    )
    .await?;

    Ok((project_for_response, Some(deployment_id)))
}

const RESERVED_VAR_PREFIXES: &[&str] = &["STACKER_", "DOCKER_", "VAULT_", "AGENT_"];

fn is_reserved_var_key(key: &str) -> bool {
    RESERVED_VAR_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
}

fn apply_install_inputs_to_deploy(
    deploy_form: &mut forms::project::Deploy,
    install_inputs: &Map<String, Value>,
) {
    if install_inputs.is_empty() {
        return;
    }
    let vars = deploy_form.stack.vars.get_or_insert_with(Vec::new);
    for (key, value) in install_inputs {
        if is_reserved_var_key(key) {
            tracing::warn!(
                "install_inputs key '{}' uses a reserved prefix and was rejected",
                key
            );
            continue;
        }
        vars.retain(|var| var.key.as_deref() != Some(key.as_str()));
        vars.push(Var {
            key: Some(key.clone()),
            value: Some(value.clone()),
        });
    }
}

fn normalized_install_inputs(inputs: &Map<String, Value>) -> Map<String, Value> {
    let mut normalized = inputs.clone();
    if let Some(value) = normalized.remove("base_domain") {
        normalized
            .entry("commonDomain".to_string())
            .or_insert(value);
    }
    if let Some(value) = normalized.remove("domain") {
        normalized
            .entry("commonDomain".to_string())
            .or_insert(value);
    }
    normalized
}

fn attach_install_inputs(request_json: &mut Value, install_inputs: &Map<String, Value>) {
    if install_inputs.is_empty() {
        return;
    }
    if let Some(custom) = request_json
        .get_mut("custom")
        .and_then(|value| value.as_object_mut())
    {
        custom.insert(
            "install_inputs".to_string(),
            Value::Object(install_inputs.clone()),
        );
    }
}

fn ensure_catalog_application_has_deploy_context(
    slug: &str,
    request: &InstallTemplateRequest,
) -> Result<()> {
    if request.deploy.is_none() {
        return Err(JsonResponse::<serde_json::Value>::build().bad_request(format!(
            "Catalog application '{}' requires deployment context; refusing to create a project without starting a deployment",
            slug
        )));
    }

    Ok(())
}

/// Redact sensitive values from a serialized `StackTemplateVersion` JSON value.
/// Covers stack_definition, config_files content, seed_jobs, and post_deploy_hooks.
fn redact_version_value(ver_value: &mut serde_json::Value, format: &str) {
    if let Some(sd) = ver_value.get_mut("stack_definition") {
        if format == "yaml" {
            if let serde_json::Value::String(yaml) = sd {
                *yaml = redact_yaml_string(yaml);
            }
        } else {
            redact_sensitive_json_values(sd);
        }
    }

    if let Some(files) = ver_value
        .get_mut("config_files")
        .and_then(|v| v.as_array_mut())
    {
        for file in files {
            if let Some(content) = file
                .get_mut("content")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                if let Some(c) = file.get_mut("content") {
                    *c = serde_json::Value::String(redact_yaml_string(&content));
                }
            }
            redact_sensitive_json_values(file);
        }
    }

    for field in &["seed_jobs", "post_deploy_hooks"] {
        if let Some(v) = ver_value.get_mut(*field) {
            redact_sensitive_json_values(v);
        }
    }
}

/// True when this install should go through per-install billing.
/// Both conditions must hold: the template opts into `per_install` and the
/// global kill switch is on. When either is false we fall through to the
/// legacy `one_time` / `free` paths untouched.
fn is_per_install_effective(template: &models::StackTemplate, settings: &Settings) -> bool {
    let opted_in = template
        .billing_cycle
        .as_deref()
        .map(|c| c.trim().eq_ignore_ascii_case("per_install"))
        .unwrap_or(false);
    opted_in && settings.per_install_billing_enabled
}

/// Convert a template's `price` (dollars, f64) to minor units (cents, i64)
/// with checked rounding. Rejects None/<=0 for per_install templates —
/// admin review is expected to prevent this from ever reaching install.
fn amount_minor_for(template: &models::StackTemplate) -> Result<i64, actix_web::Error> {
    let price = template.price.unwrap_or(0.0);
    if price <= 0.0 {
        return Err(
            JsonResponse::<serde_json::Value>::build().bad_request(format!(
                "Template '{}' has billing_cycle=per_install but no positive price",
                template.slug
            )),
        );
    }
    let cents = (price * 100.0).round() as i64;
    if cents <= 0 {
        return Err(
            JsonResponse::<serde_json::Value>::build().bad_request(format!(
                "Template '{}' price rounds to zero cents",
                template.slug
            )),
        );
    }
    Ok(cents)
}

/// Best-effort background void of an authorization on install failure.
/// Fire-and-forget: the sweeper is the correctness backstop if this
/// spawn is dropped or the user_service call fails.
fn spawn_void(
    user_service: Arc<dyn UserServiceConnector>,
    pg_pool: PgPool,
    user_token: String,
    authorization_id: String,
    reason: String,
) {
    tokio::spawn(async move {
        if let Err(err) = user_service
            .void_install_charge(&user_token, &authorization_id, &reason)
            .await
        {
            tracing::warn!(
                "per_install void failed for {}: {} (sweeper will retry)",
                authorization_id,
                err
            );
        }
        if let Err(err) =
            db::marketplace_billing::mark_voided(&pg_pool, &authorization_id, &reason).await
        {
            tracing::warn!(
                "per_install DB void mark failed for {}: {}",
                authorization_id,
                err
            );
        }
    });
}

fn summarize(
    handle: &crate::connectors::user_service::AuthorizationHandle,
) -> AuthorizationSummary {
    AuthorizationSummary {
        authorization_id: handle.authorization_id.clone(),
        status: handle.status.clone(),
        amount_minor: handle.amount_minor,
        currency: handle.currency.clone(),
        expires_at: handle.expires_at.clone(),
    }
}

async fn install_stack_template(
    template: models::StackTemplate,
    latest_version: models::StackTemplateVersion,
    request: &InstallTemplateRequest,
    user: &models::User,
    pg_pool: &PgPool,
    mq_manager: &MqManager,
    settings: &Settings,
    user_service: &Arc<dyn UserServiceConnector>,
    install_service: &Arc<dyn InstallServiceConnector>,
    vault_client: &VaultClient,
) -> Result<InstallTemplateResponse> {
    services::validate_marketplace_template_access(user_service, user, &template)
        .await
        .map_err(map_access_error)?;

    // Per-install billing: authorize before any DB write so a decline
    // returns 402 with no side effects.
    let per_install = is_per_install_effective(&template, settings);
    let (mut authorization_state, effective_idem_key) = if per_install {
        let user_token = user.access_token.as_deref().ok_or_else(|| {
            JsonResponse::<serde_json::Value>::build()
                .forbidden("User token required for per-install billing")
        })?;
        let amount_minor = amount_minor_for(&template)?;
        let currency = template
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());
        let idempotency_key = request
            .idempotency_key
            .clone()
            .filter(|k| !k.trim().is_empty())
            .unwrap_or_else(|| {
                let key = format!("srv-{}", uuid::Uuid::new_v4());
                tracing::warn!(
                    "install request for per_install template '{}' missing idempotency_key; generated '{}'",
                    template.slug,
                    key
                );
                key
            });

        let handle = user_service
            .authorize_install_charge(
                user_token,
                &template.id,
                amount_minor,
                &currency,
                &idempotency_key,
            )
            .await
            .map_err(|err| match err {
                ConnectorError::PaymentRequired(msg) => JsonResponse::<serde_json::Value>::build()
                    .payment_required(format!("Payment declined: {}", msg)),
                ConnectorError::Conflict(msg) => JsonResponse::<serde_json::Value>::build()
                    .bad_request(format!("Idempotency conflict: {}", msg)),
                other => {
                    tracing::error!("authorize_install_charge failed: {:?}", other);
                    JsonResponse::<serde_json::Value>::build()
                        .internal_server_error("Failed to authorize install charge")
                }
            })?;

        // Persist the authorization row before touching the project row so
        // a crash between authorize and project insert still leaves us an
        // auditable ledger entry for the sweeper.
        let expires_at = handle
            .expires_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));
        let auth_row = db::marketplace_billing::insert_authorization(
            pg_pool,
            db::marketplace_billing::NewAuthorization {
                user_id: user.id.clone(),
                template_id: template.id,
                idempotency_key: idempotency_key.clone(),
                authorization_id: handle.authorization_id.clone(),
                amount_minor: handle.amount_minor,
                currency: handle.currency.clone(),
                expires_at,
            },
        )
        .await
        .map_err(|err| {
            tracing::error!("insert_authorization failed: {}", err);
            spawn_void(
                user_service.clone(),
                pg_pool.clone(),
                user_token.to_string(),
                handle.authorization_id.clone(),
                "install_failed:db_insert_authorization".to_string(),
            );
            JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Failed to persist install authorization")
        })?;

        (
            Some((auth_row, handle, user_token.to_string())),
            Some(idempotency_key),
        )
    } else {
        (None, None)
    };

    // Any error from here forward must void the authorization if one was
    // taken. We keep the flow linear and handle each `?` explicitly via a
    // small helper that voids on failure paths.
    let form = match build_project_form(&template, &latest_version, request.name.as_deref()) {
        Ok(f) => f,
        Err(e) => {
            if let Some((_, handle, token)) = authorization_state.take() {
                spawn_void(
                    user_service.clone(),
                    pg_pool.clone(),
                    token,
                    handle.authorization_id,
                    "install_failed:build_project_form".to_string(),
                );
            }
            return Err(e);
        }
    };

    let mut request_json = serde_json::to_value(&form).map_err(|err| {
        tracing::error!("Failed to serialize marketplace project form: {:?}", err);
        if let Some((_, handle, token)) = authorization_state.take() {
            spawn_void(
                user_service.clone(),
                pg_pool.clone(),
                token,
                handle.authorization_id,
                "install_failed:serialize_form".to_string(),
            );
        }
        JsonResponse::<serde_json::Value>::build().internal_server_error("Internal Server Error")
    })?;
    let install_inputs = normalized_install_inputs(&request.install_inputs);
    attach_install_inputs(&mut request_json, &install_inputs);

    let mut project = match insert_project_from_form(pg_pool, user, &form, request_json).await {
        Ok(p) => p,
        Err(e) => {
            if let Some((_, handle, token)) = authorization_state.take() {
                spawn_void(
                    user_service.clone(),
                    pg_pool.clone(),
                    token,
                    handle.authorization_id,
                    "install_failed:insert_project".to_string(),
                );
            }
            return Err(e);
        }
    };
    project.source_template_id = Some(template.id);
    project.template_version = Some(latest_version.version.clone());
    project = db::project::update(pg_pool, project).await.map_err(|err| {
        tracing::error!("Failed to update installed template metadata: {}", err);
        JsonResponse::<serde_json::Value>::build().internal_server_error("Internal Server Error")
    })?;

    // Link the auth row to the freshly-committed project so
    // deploy_complete_handler and cancel/refund paths can find it.
    if let Some((auth_row, _handle, _token)) = authorization_state.as_ref() {
        if let Err(err) =
            db::marketplace_billing::attach_project(pg_pool, auth_row.id, project.id).await
        {
            tracing::warn!(
                "attach_project failed for auth {}: {} (row still usable via idempotency_key)",
                auth_row.id,
                err
            );
        }
    }

    let slug = template.slug.clone();
    let (project, deployment_id) = maybe_deploy_installed_project(
        user,
        project,
        request.deploy.clone(),
        &slug,
        pg_pool,
        mq_manager,
        settings,
        user_service,
        install_service,
        vault_client,
        &install_inputs,
    )
    .await?;

    // Attach deployment_hash if a deployment was queued. If no deploy was
    // requested (sync install-only) capture immediately — the value the
    // buyer paid for is the install artifact itself.
    if let Some((auth_row, handle, token)) = authorization_state.as_mut() {
        if let Some(deploy_id) = deployment_id {
            match db::deployment::fetch(pg_pool, deploy_id).await {
                Ok(Some(deployment)) => {
                    if let Err(err) = db::marketplace_billing::attach_deployment_hash(
                        pg_pool,
                        auth_row.id,
                        &deployment.deployment_hash,
                    )
                    .await
                    {
                        tracing::warn!(
                            "attach_deployment_hash failed for auth {}: {}",
                            auth_row.id,
                            err
                        );
                    }
                }
                Ok(None) => tracing::warn!(
                    "deployment id {} not found when linking auth {}",
                    deploy_id,
                    auth_row.id
                ),
                Err(err) => tracing::warn!("deployment lookup failed for {}: {}", deploy_id, err),
            }
        } else {
            // Install-without-deploy: synthesize a hash and capture now.
            let synthetic = format!("install-only:{}", project.id);
            let _ =
                db::marketplace_billing::attach_deployment_hash(pg_pool, auth_row.id, &synthetic)
                    .await;
            match user_service
                .capture_install_charge(token, &handle.authorization_id, &synthetic)
                .await
            {
                Ok(new_handle) => {
                    let _ =
                        db::marketplace_billing::mark_captured(pg_pool, &handle.authorization_id)
                            .await;
                    *handle = new_handle;
                }
                Err(err) => {
                    tracing::warn!(
                        "sync install-only capture failed for {}: {} (sweeper will reconcile)",
                        handle.authorization_id,
                        err
                    );
                }
            }
        }
    }

    let format = latest_version
        .definition_format
        .as_deref()
        .unwrap_or("")
        .to_string();
    let mut ver_value =
        serde_json::to_value(latest_version).unwrap_or_else(|_| serde_json::json!({}));

    redact_version_value(&mut ver_value, &format);

    let (authorization, idempotency_key) = match (authorization_state, effective_idem_key) {
        (Some((_, handle, _)), Some(key)) => (Some(summarize(&handle)), Some(key)),
        _ => (None, None),
    };

    Ok(InstallTemplateResponse {
        project,
        template: serde_json::to_value(template).unwrap_or_else(|_| serde_json::json!({})),
        latest_version: ver_value,
        deployment_id,
        authorization,
        idempotency_key,
    })
}

async fn install_catalog_application(
    slug: &str,
    request: &InstallTemplateRequest,
    user: &models::User,
    pg_pool: &PgPool,
    mq_manager: &MqManager,
    settings: &Settings,
    user_service: &Arc<dyn UserServiceConnector>,
    install_service: &Arc<dyn InstallServiceConnector>,
    vault_client: &VaultClient,
) -> Result<InstallTemplateResponse> {
    ensure_catalog_application_has_deploy_context(slug, request)?;

    let token = user.access_token.as_deref().ok_or_else(|| {
        JsonResponse::<serde_json::Value>::build()
            .forbidden("User token is required to install catalog applications")
    })?;

    let applications = user_service
        .search_marketplace_templates(token, Some(slug), None, None, Some(1), Some(10))
        .await
        .map_err(|err| {
            tracing::error!(
                "Catalog application lookup failed for '{}': {:?}",
                slug,
                err
            );
            JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Catalog application lookup failed")
        })?;

    let slug_lc = slug.to_ascii_lowercase();
    let mut application = applications
        .into_iter()
        .find(|application| catalog_application_matches_slug(application, &slug_lc))
        .ok_or_else(|| {
            JsonResponse::<serde_json::Value>::build().not_found(format!(
                "Template or catalog application '{}' was not found",
                slug
            ))
        })?;

    // The search result is a summary and may lack the docker image / default
    // env & ports. Enrich from the catalog endpoint so we can synthesize a
    // deployable compose; missing enrichment is non-fatal (best effort).
    match user_service.get_catalog_application(token, slug).await {
        Ok(Some(enriched)) => merge_catalog_enrichment(&mut application, &enriched),
        Ok(None) => {}
        Err(err) => {
            tracing::warn!(
                "Catalog enrichment lookup failed for '{}': {:?} (continuing with summary)",
                slug,
                err
            );
        }
    }

    let form = catalog_application_project_form(&application, slug, request.name.as_deref())?;
    let mut request_json = serde_json::to_value(&form).map_err(|err| {
        tracing::error!(
            "Failed to serialize catalog application project form: {:?}",
            err
        );
        JsonResponse::<serde_json::Value>::build().internal_server_error("Internal Server Error")
    })?;
    if let Some(custom) = request_json
        .get_mut("custom")
        .and_then(|value| value.as_object_mut())
    {
        custom.insert("catalog_application".to_string(), application.clone());
    }
    let install_inputs = normalized_install_inputs(&request.install_inputs);
    attach_install_inputs(&mut request_json, &install_inputs);

    let project = insert_project_from_form(pg_pool, user, &form, request_json).await?;
    let (project, deployment_id) = maybe_deploy_installed_project(
        user,
        project,
        request.deploy.clone(),
        slug,
        pg_pool,
        mq_manager,
        settings,
        user_service,
        install_service,
        vault_client,
        &install_inputs,
    )
    .await?;

    Ok(InstallTemplateResponse {
        authorization: None,
        idempotency_key: None,
        project,
        template: application,
        latest_version: serde_json::json!({
            "version": "catalog",
            "stack_definition": null,
            "rendered_by": "install_service"
        }),
        deployment_id,
    })
}

fn catalog_application_matches_slug(application: &Value, slug_lc: &str) -> bool {
    ["code", "slug"].iter().any(|field| {
        application
            .get(field)
            .and_then(|value| value.as_str())
            .map(|value| value.to_ascii_lowercase() == slug_lc)
            .unwrap_or(false)
    })
}

#[tracing::instrument(name = "Install marketplace template", skip_all)]
#[post("/{slug}/install")]
pub async fn install_handler(
    path: web::Path<(String,)>,
    request: web::Json<InstallTemplateRequest>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
    mq_manager: web::Data<MqManager>,
    settings: web::Data<Settings>,
    user_service: web::Data<Arc<dyn UserServiceConnector>>,
    install_service: web::Data<Arc<dyn InstallServiceConnector>>,
    vault_client: web::Data<VaultClient>,
) -> Result<impl Responder> {
    let slug = path.into_inner().0;
    let response = match db::marketplace::get_by_slug_with_latest(pg_pool.get_ref(), &slug).await {
        Ok((template, Some(latest_version))) => {
            install_stack_template(
                template,
                latest_version,
                &request,
                user.as_ref(),
                pg_pool.get_ref(),
                mq_manager.get_ref(),
                settings.get_ref(),
                user_service.get_ref(),
                install_service.get_ref(),
                vault_client.get_ref(),
            )
            .await?
        }
        Ok((template, None)) => {
            return Err(
                JsonResponse::<serde_json::Value>::build().bad_request(format!(
                    "Template '{}' has no installable version",
                    template.slug
                )),
            );
        }
        Err(db::marketplace::SlugLookupError::Internal) => {
            return Err(JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Internal Server Error"));
        }
        Err(db::marketplace::SlugLookupError::NotFound) => {
            install_catalog_application(
                &slug,
                &request,
                user.as_ref(),
                pg_pool.get_ref(),
                mq_manager.get_ref(),
                settings.get_ref(),
                user_service.get_ref(),
                install_service.get_ref(),
                vault_client.get_ref(),
            )
            .await?
        }
    };

    let message = if response.deployment_id.is_some() {
        "Install deployment started"
    } else {
        "Template installed"
    };

    Ok(JsonResponse::build().set_item(response).ok(message))
}

#[cfg(test)]
mod tests {
    use super::{
        amount_minor_for, build_project_form, catalog_application_project_form,
        ensure_catalog_application_has_deploy_context, is_per_install_effective,
        merge_catalog_enrichment, summarize, synthesize_catalog_compose, validate_installable_form,
    };
    use crate::configuration::Settings;
    use crate::connectors::user_service::AuthorizationHandle;
    use crate::{forms::project::Payload, models};
    use serde_json::{json, Map};
    use uuid::Uuid;

    fn make_yaml_version() -> models::StackTemplateVersion {
        models::StackTemplateVersion {
            id: Uuid::new_v4(),
            template_id: Uuid::new_v4(),
            version: "1.0.0".to_string(),
            stack_definition: json!("version: '3.8'\nservices:\n  app:\n    image: nginx:latest"),
            config_files: json!([]),
            assets: json!([]),
            seed_jobs: json!([]),
            post_deploy_hooks: json!([]),
            update_mode_capabilities: None,
            definition_format: Some("yaml".to_string()),
            changelog: None,
            is_latest: Some(true),
            created_at: None,
        }
    }

    #[test]
    fn yaml_stack_definition_builds_valid_project_form() {
        let template = models::StackTemplate {
            slug: "stackdog".to_string(),
            name: "Stackdog".to_string(),
            short_description: Some("A monitoring tool".to_string()),
            category_code: Some("monitoring".to_string()),
            ..Default::default()
        };
        let version = make_yaml_version();

        let form = build_project_form(&template, &version, None)
            .expect("YAML stack should produce a valid ProjectForm");

        assert_eq!(form.custom.custom_stack_code, "stackdog");
        assert_eq!(form.custom.project_name, Some("Stackdog".to_string()));
        assert_eq!(
            form.custom.custom_stack_short_description,
            Some("A monitoring tool".to_string())
        );
        assert!(form.custom.web.is_empty());
        assert_eq!(form.custom.service.as_ref().map(|v| v.len()), Some(0));
        assert_eq!(form.custom.feature.as_ref().map(|v| v.len()), Some(0));
        assert_eq!(form.custom.marketplace_version, Some("1.0.0".to_string()));
    }

    #[test]
    fn yaml_stack_definition_embeds_compose_in_config_files() {
        let template = models::StackTemplate {
            slug: "stackdog".to_string(),
            name: "Stackdog".to_string(),
            ..Default::default()
        };
        let version = make_yaml_version();

        let form = build_project_form(&template, &version, None)
            .expect("YAML stack should produce a valid ProjectForm");

        let config_files: &Vec<serde_json::Value> = form
            .custom
            .marketplace_config_files
            .as_array()
            .expect("config_files should be an array");

        let has_compose = config_files.iter().any(|f| {
            f.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.contains("docker-compose") || n.contains("compose"))
                .unwrap_or(false)
        });
        assert!(has_compose, "config_files should contain a compose entry");
    }

    #[test]
    fn yaml_stack_definition_respects_requested_name() {
        let template = models::StackTemplate {
            slug: "stackdog".to_string(),
            name: "Stackdog".to_string(),
            ..Default::default()
        };
        let version = make_yaml_version();

        let form = build_project_form(&template, &version, Some("my-instance"))
            .expect("YAML stack should produce a valid ProjectForm");

        assert_eq!(form.custom.custom_stack_code, "my-instance");
        assert_eq!(form.custom.project_name, Some("Stackdog".to_string()));
    }

    #[test]
    fn catalog_application_project_form_preserves_catalog_context() {
        let application = json!({
            "code": "dify",
            "name": "Dify",
            "description": "Dify AI application platform",
            "categories": ["AI"],
            // A single-app catalog entry carries a real container image; without
            // one it is not installable (catalog_application_project_form now
            // fails loud rather than shipping an empty compose).
            "docker_image": "langgenius/dify-api",
            "is_from_marketplace": true
        });

        let form = catalog_application_project_form(&application, "dify", None)
            .expect("catalog application should produce a valid project form");
        let metadata = serde_json::to_value(&form).expect("form should serialize");
        let project = models::Project::new(
            "user-1".to_string(),
            form.custom.custom_stack_code.clone(),
            metadata,
            json!({}),
        );
        let payload = Payload::try_from(&project).expect("project metadata should build payload");

        assert_eq!(form.custom.custom_stack_code, "dify");
        assert_eq!(form.custom.project_name.as_deref(), Some("Dify"));
        assert_eq!(payload.custom.catalog_application["code"], json!("dify"));
        assert_eq!(
            payload.custom.catalog_application["is_from_marketplace"],
            json!(true)
        );
    }

    #[test]
    fn synthesize_catalog_compose_builds_single_service_from_image_and_port() {
        let application = json!({
            "code": "n8n",
            "docker_image": "n8nio/n8n:latest",
            "default_port": 5678,
            "default_env": { "N8N_PORT": "5678", "GENERIC_TIMEZONE": "UTC" }
        });

        let compose = synthesize_catalog_compose(&application, "n8n")
            .expect("compose should be synthesized when a docker image is present");
        let parsed: serde_yaml::Value = serde_yaml::from_str(&compose).unwrap();
        let svc = &parsed["services"]["n8n"];

        assert_eq!(svc["image"], serde_yaml::Value::from("n8nio/n8n:latest"));
        assert_eq!(svc["ports"][0], serde_yaml::Value::from("5678:5678"));
        assert_eq!(
            svc["environment"]["N8N_PORT"],
            serde_yaml::Value::from("5678")
        );
        assert_eq!(svc["restart"], serde_yaml::Value::from("unless-stopped"));
        // External default_network is declared so the stack joins the shared net.
        assert_eq!(
            parsed["networks"]["default_network"]["external"],
            serde_yaml::Value::from(true)
        );
    }

    #[test]
    fn synthesize_catalog_compose_returns_none_without_image() {
        let application = json!({ "code": "n8n", "default_port": 5678 });
        assert!(synthesize_catalog_compose(&application, "n8n").is_none());
    }

    #[test]
    fn synthesize_catalog_compose_accepts_kv_list_env() {
        let application = json!({
            "docker_image": "img:1",
            "default_env": [ { "key": "A", "value": "1" }, "B=2" ]
        });
        let compose = synthesize_catalog_compose(&application, "svc").unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&compose).unwrap();
        assert_eq!(
            parsed["services"]["svc"]["environment"]["A"],
            serde_yaml::Value::from("1")
        );
        assert_eq!(
            parsed["services"]["svc"]["environment"]["B"],
            serde_yaml::Value::from("2")
        );
    }

    #[test]
    fn synthesize_catalog_compose_builds_multi_service_stack_from_members() {
        // A stack (e.g. LAMP) carries no top-level image; each member service
        // provides its own real container image. The synthesizer must emit one
        // compose service per member, keyed by member code, and never fall back
        // to the empty stack-level docker_image.
        let application = json!({
            "code": "lamp",
            "kind": "stack",
            "docker_image": "",
            "services": [
                {
                    "code": "apache",
                    "docker_image": "trydirect/php",
                    "default_ports": [80],
                    "default_env": { "PHP_ENV": "prod" }
                },
                {
                    "code": "mariadb",
                    "docker_image": "mariadb:11",
                    "default_env": [ { "key": "MARIADB_ROOT_PASSWORD", "value": "secret" } ]
                }
            ]
        });

        let compose = synthesize_catalog_compose(&application, "lamp")
            .expect("a stack with member images should synthesize a multi-service compose");
        let parsed: serde_yaml::Value = serde_yaml::from_str(&compose).unwrap();
        let services = parsed["services"].as_mapping().expect("services mapping");

        // One service per member, keyed by member code (not the stack code).
        assert_eq!(services.len(), 2);
        assert_eq!(
            parsed["services"]["apache"]["image"],
            serde_yaml::Value::from("trydirect/php")
        );
        assert_eq!(
            parsed["services"]["apache"]["ports"][0],
            serde_yaml::Value::from("80:80")
        );
        assert_eq!(
            parsed["services"]["apache"]["environment"]["PHP_ENV"],
            serde_yaml::Value::from("prod")
        );
        assert_eq!(
            parsed["services"]["mariadb"]["image"],
            serde_yaml::Value::from("mariadb:11")
        );
        assert_eq!(
            parsed["services"]["mariadb"]["environment"]["MARIADB_ROOT_PASSWORD"],
            serde_yaml::Value::from("secret")
        );
        // The empty stack-level docker_image must never leak in as a service.
        assert!(services.get(serde_yaml::Value::from("lamp")).is_none());
    }

    #[test]
    fn synthesize_catalog_compose_returns_none_for_stack_without_member_images() {
        // A stack whose members all lack images cannot be synthesized -> None,
        // so the caller fails loud rather than shipping an empty compose.
        let application = json!({
            "kind": "stack",
            "docker_image": "",
            "services": [ { "code": "apache" }, { "code": "mariadb" } ]
        });
        assert!(synthesize_catalog_compose(&application, "lamp").is_none());
    }

    #[test]
    fn full_production_lamp_payload_deserializes_and_synthesizes_13_services() {
        // The EXACT bytes returned by GET /applications/catalog/lamp in production
        // (all 13 members). Proves the full response parses into Application, that
        // services[] survives the get_catalog_application round-trip, and that the
        // synthesizer emits a multi-service compose. If this passes, a runtime
        // "single-app fail-loud" cannot be a deserialization problem.
        let raw = include_str!("lamp_catalog_fixture.json");

        let app: crate::connectors::user_service::app::Application = serde_json::from_str(raw)
            .expect("full production LAMP payload must deserialize into Application");
        let value = serde_json::to_value(&app).expect("Application must reserialize");

        let services = value
            .get("services")
            .and_then(|v| v.as_array())
            .expect("services[] must survive the Application round-trip");
        assert_eq!(services.len(), 13, "expected all 13 members to survive");

        let compose = synthesize_catalog_compose(&value, "lamp")
            .expect("full payload must synthesize a multi-service compose");
        for img in ["trydirect/mysql", "trydirect/apache", "trydirect/nginx-proxy-manager"] {
            assert!(compose.contains(img), "compose missing {img}:\n{compose}");
        }
    }

    #[test]
    fn merge_catalog_enrichment_carries_services_into_synthesized_stack() {
        // Reproduces the real install_catalog_application flow: a service-less
        // search summary is enriched from /applications/catalog, then synthesized.
        // The stack's services[]/kind must survive merge_catalog_enrichment,
        // otherwise synthesize wrongly takes the single-app path and fails loud.
        let enriched: serde_json::Value =
            serde_json::from_str(include_str!("lamp_catalog_fixture.json")).unwrap();
        let mut summary = json!({ "code": "lamp", "name": "LAMP", "is_from_marketplace": true });

        merge_catalog_enrichment(&mut summary, &enriched);

        assert_eq!(summary.get("kind").and_then(|v| v.as_str()), Some("stack"));
        let services = summary
            .get("services")
            .and_then(|v| v.as_array())
            .expect("services[] must be carried into the summary by the merge");
        assert_eq!(services.len(), 13);

        let compose = synthesize_catalog_compose(&summary, "lamp")
            .expect("merged stack summary must synthesize a multi-service compose");
        assert!(compose.contains("trydirect/mysql"), "compose:\n{compose}");
        assert!(compose.contains("trydirect/apache"), "compose:\n{compose}");
    }

    #[test]
    fn real_stack_catalog_payload_round_trips_and_synthesizes_multi_service() {
        // Exact shape returned by GET /applications/catalog/lamp in production:
        // kind:"stack", empty top-level docker_image, member services[] with real
        // images, null/list default_ports, object default_env. This reproduces the
        // full server path: fetch_app_catalog deserializes into Application, then
        // get_catalog_application reserializes via serde_json::to_value, then the
        // synthesizer runs. `services[]` must survive the round-trip.
        let raw = r#"{
            "_id": 54, "code": "lamp", "name": "LAMP", "kind": "stack",
            "docker_image": "",
            "services": [
                {"_id":1,"code":"mysql","name":"MySQL","role":null,"type":"service",
                 "docker_image":"trydirect/mysql","default_ports":null,
                 "default_env":{"MYSQL_HOST":"mysqldb","MYSQL_PORT":"3306"},
                 "default_config_files":[]},
                {"_id":9,"code":"statuspanel","name":"Status Panel","role":"statuspanel",
                 "type":"feature","docker_image":"trydirect/status",
                 "default_ports":["5000"],"default_env":{},"default_config_files":[]}
            ]
        }"#;

        let app: crate::connectors::user_service::app::Application =
            serde_json::from_str(raw).expect("catalog stack payload must deserialize into Application");
        let value = serde_json::to_value(&app).expect("Application must reserialize");

        // The critical invariant: services[] survives the typed round-trip.
        let services = value
            .get("services")
            .and_then(|v| v.as_array())
            .expect("services[] must survive the Application round-trip");
        assert_eq!(services.len(), 2, "round-trip dropped services: {value}");

        let compose = synthesize_catalog_compose(&value, "lamp")
            .expect("stack payload must synthesize a multi-service compose, not fail loud");
        assert!(compose.contains("trydirect/mysql"), "compose: {compose}");
        assert!(compose.contains("trydirect/status"), "compose: {compose}");
    }

    #[test]
    fn catalog_form_embeds_synthesized_compose_when_image_present() {
        let application = json!({
            "code": "n8n",
            "name": "n8n",
            "docker_image": "n8nio/n8n:latest",
            "default_port": 5678
        });

        let form = catalog_application_project_form(&application, "n8n", None).unwrap();
        let files = &form.custom.marketplace_config_files;
        let arr = files
            .as_array()
            .expect("marketplace_config_files should be an array");
        let compose = arr
            .iter()
            .find(|f| f.get("name").and_then(|n| n.as_str()) == Some("docker-compose.yml"))
            .and_then(|f| f.get("content"))
            .and_then(|c| c.as_str())
            .expect("a docker-compose.yml should be embedded");
        assert!(compose.contains("n8nio/n8n:latest"));
        assert!(compose.contains("services:"));
    }

    #[test]
    fn merge_catalog_enrichment_overlays_deployable_fields() {
        let mut application = json!({ "code": "n8n", "name": "n8n" });
        let enriched = json!({
            "docker_image": "n8nio/n8n:latest",
            "default_port": 5678,
            "default_env": null
        });
        merge_catalog_enrichment(&mut application, &enriched);

        assert_eq!(application["docker_image"], json!("n8nio/n8n:latest"));
        assert_eq!(application["default_port"], json!(5678));
        // Null enriched values must not overwrite / introduce nulls.
        assert!(application.get("default_env").is_none());
    }

    #[test]
    fn catalog_application_slug_match_accepts_code_or_slug() {
        assert!(super::catalog_application_matches_slug(
            &json!({ "code": "dify" }),
            "dify"
        ));
        assert!(super::catalog_application_matches_slug(
            &json!({ "slug": "dify" }),
            "dify"
        ));
        assert!(!super::catalog_application_matches_slug(
            &json!({ "code": "wordpress" }),
            "dify"
        ));
    }

    #[test]
    fn catalog_application_requires_deploy_context_before_project_creation() {
        let request = super::InstallTemplateRequest {
            name: None,
            deploy: None,
            install_inputs: Map::new(),
            idempotency_key: None,
        };

        assert!(ensure_catalog_application_has_deploy_context("dify", &request).is_err());
    }

    // Production's ghost: the exact shape `stacker submit` sends — the raw
    // stacker.yml parsed to JSON (has `app`/`services`/`name`, no `custom`
    // wrapper, `definition_format` unset). Install must accept this and
    // embed the serialized YAML as `stacker.yml` in marketplace_config_files.
    #[test]
    fn build_project_form_accepts_stacker_yml_shape_from_submit() {
        let template = models::StackTemplate {
            slug: "ghost".to_string(),
            name: "ghost".to_string(),
            short_description: Some("Ghost blog".to_string()),
            category_code: Some("cms".to_string()),
            ..Default::default()
        };
        let version = models::StackTemplateVersion {
            id: Uuid::new_v4(),
            template_id: Uuid::new_v4(),
            version: "1.0.0".to_string(),
            stack_definition: json!({
                "name": "ghost",
                "app": { "type": "custom", "image": "ghost:5-alpine" },
                "services": [],
                "deploy": { "target": "local" }
            }),
            config_files: json!([]),
            assets: json!([]),
            seed_jobs: json!([]),
            post_deploy_hooks: json!([]),
            update_mode_capabilities: None,
            definition_format: None,
            changelog: None,
            is_latest: Some(true),
            created_at: None,
        };

        let form = build_project_form(&template, &version, None)
            .expect("stacker.yml shape from `stacker submit` must build a ProjectForm");
        assert_eq!(form.custom.custom_stack_code, "ghost");
        assert_eq!(form.custom.project_name, Some("ghost".to_string()));

        let files: &Vec<serde_json::Value> = form
            .custom
            .marketplace_config_files
            .as_array()
            .expect("marketplace_config_files should be an array");
        let embed = files
            .iter()
            .find(|f| f.get("name").and_then(|n| n.as_str()) == Some("stacker.yml"))
            .expect(
                "stacker.yml shape should be embedded as `stacker.yml`, not `docker-compose.yml`",
            );
        let content = embed
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or_default();
        assert!(
            content.contains("app:") && content.contains("ghost:5-alpine"),
            "embedded content should be the serialized stacker.yml, got: {}",
            content
        );

        validate_installable_form(&template, &version)
            .expect("validator should agree with build_project_form");
    }

    #[test]
    fn validate_installable_accepts_yaml_compose_definition() {
        let template = models::StackTemplate {
            slug: "umami".to_string(),
            name: "umami".to_string(),
            ..Default::default()
        };
        let version = make_yaml_version();

        validate_installable_form(&template, &version)
            .expect("a compose-YAML stack definition must pass the gate");
    }

    #[test]
    fn validate_installable_accepts_legacy_custom_wrapped_definition() {
        let template = models::StackTemplate {
            slug: "legacy".to_string(),
            name: "legacy".to_string(),
            ..Default::default()
        };
        let version = models::StackTemplateVersion {
            id: Uuid::new_v4(),
            template_id: Uuid::new_v4(),
            version: "1.0.0".to_string(),
            stack_definition: json!({
                "custom": {
                    "custom_stack_code": "legacy",
                    "web": [],
                    "networks": []
                }
            }),
            config_files: json!([]),
            assets: json!([]),
            seed_jobs: json!([]),
            post_deploy_hooks: json!([]),
            update_mode_capabilities: None,
            definition_format: None,
            changelog: None,
            is_latest: Some(true),
            created_at: None,
        };

        validate_installable_form(&template, &version)
            .expect("a legacy custom-wrapped stack definition must pass the gate");
    }
    #[test]
    fn reserved_prefix_vars_rejected_in_install_inputs() {
        let mut deploy = crate::forms::project::Deploy::default();
        let mut inputs = Map::new();
        inputs.insert("VAULT_TOKEN".to_string(), json!("secret-vault"));
        inputs.insert("STACKER_SECRET".to_string(), json!("stacker-internal"));
        inputs.insert(
            "DOCKER_HOST".to_string(),
            json!("unix:///var/run/docker.sock"),
        );
        inputs.insert("AGENT_KEY".to_string(), json!("agent-secret"));
        inputs.insert("normal_key".to_string(), json!("safe-value"));

        super::apply_install_inputs_to_deploy(&mut deploy, &inputs);

        let vars = deploy
            .stack
            .vars
            .as_ref()
            .expect("vars should be populated");
        let keys: Vec<&str> = vars.iter().filter_map(|v| v.key.as_deref()).collect();

        for key in &keys {
            assert!(
                !key.starts_with("VAULT_"),
                "VAULT_ key must be rejected but found: {}",
                key
            );
            assert!(
                !key.starts_with("STACKER_"),
                "STACKER_ key must be rejected but found: {}",
                key
            );
            assert!(
                !key.starts_with("DOCKER_"),
                "DOCKER_ key must be rejected but found: {}",
                key
            );
            assert!(
                !key.starts_with("AGENT_"),
                "AGENT_ key must be rejected but found: {}",
                key
            );
        }

        assert!(
            keys.contains(&"normal_key"),
            "normal_key should pass through to vars"
        );
    }

    #[test]
    fn response_config_files_are_redacted() {
        let mut version = make_yaml_version();
        // Use YAML format (compose files) — redact_yaml_string redacts by key name.
        version.config_files = json!([
            {
                "name": "docker-compose.yml",
                "content": "services:\n  app:\n    image: nginx\n    environment:\n      SECRET_KEY: abc123secretvalue\n      DB_PASSWORD: hunter2\n"
            }
        ]);
        version.stack_definition =
            json!("SECRET_KEY: supersecret\nservices:\n  app:\n    image: nginx");
        version.definition_format = Some("yaml".to_string());

        let format = version
            .definition_format
            .as_deref()
            .unwrap_or("")
            .to_string();
        let mut ver_value = serde_json::to_value(&version).unwrap();
        super::redact_version_value(&mut ver_value, &format);

        if let Some(files) = ver_value["config_files"].as_array() {
            for file in files {
                let content = file["content"].as_str().unwrap_or("");
                assert!(
                    !content.contains("abc123secretvalue"),
                    "config_files content must not expose raw secret values (found 'abc123secretvalue')"
                );
                assert!(
                    !content.contains("hunter2"),
                    "config_files content must not expose raw secret values (found 'hunter2')"
                );
            }
        }
    }

    // ── per_install helpers (pure functions) ─────────────────────────
    //
    // Full install-handler integration tests live under `tests/` — they
    // need a real Postgres + wiremock and belong with the cucumber suite.
    // The tests here pin the pure pieces that decide *whether* to bill and
    // *how much*, plus the response-shape summarizer.

    fn per_install_template_fixture() -> models::StackTemplate {
        models::StackTemplate {
            slug: "paid-per-install".to_string(),
            price: Some(9.99),
            billing_cycle: Some("per_install".to_string()),
            currency: Some("USD".to_string()),
            product_id: Some(42),
            ..Default::default()
        }
    }

    fn settings_with_billing_flag(enabled: bool) -> Settings {
        let mut s = Settings::default();
        s.per_install_billing_enabled = enabled;
        s
    }

    #[test]
    fn is_per_install_effective_requires_both_opt_in_and_flag() {
        let template = per_install_template_fixture();

        assert!(is_per_install_effective(
            &template,
            &settings_with_billing_flag(true)
        ));
        assert!(
            !is_per_install_effective(&template, &settings_with_billing_flag(false)),
            "kill switch off must force the gate to fall through to legacy paths"
        );

        // Template not opted in but flag on — still not per_install.
        let mut legacy = template.clone();
        legacy.billing_cycle = Some("one_time".to_string());
        assert!(!is_per_install_effective(
            &legacy,
            &settings_with_billing_flag(true)
        ));

        // billing_cycle missing entirely — not per_install.
        legacy.billing_cycle = None;
        assert!(!is_per_install_effective(
            &legacy,
            &settings_with_billing_flag(true)
        ));
    }

    #[test]
    fn amount_minor_for_rounds_and_rejects_non_positive() {
        let mut t = per_install_template_fixture();

        t.price = Some(9.99);
        assert_eq!(amount_minor_for(&t).unwrap(), 999);

        t.price = Some(10.005);
        assert_eq!(
            amount_minor_for(&t).unwrap(),
            1001,
            "banker rounding to nearest cent"
        );

        // Zero and negative and missing are rejected up front — admin
        // review should catch these before install, but we defensively
        // fail-closed here anyway.
        t.price = Some(0.0);
        assert!(amount_minor_for(&t).is_err());
        t.price = Some(-1.0);
        assert!(amount_minor_for(&t).is_err());
        t.price = None;
        assert!(amount_minor_for(&t).is_err());
    }

    #[test]
    fn summarize_flattens_authorization_handle_for_response() {
        let h = AuthorizationHandle {
            authorization_id: "auth-xyz".to_string(),
            amount_minor: 999,
            currency: "USD".to_string(),
            expires_at: Some("2099-01-01T00:00:00Z".to_string()),
            status: "authorized".to_string(),
        };
        let s = summarize(&h);
        assert_eq!(s.authorization_id, "auth-xyz");
        assert_eq!(s.amount_minor, 999);
        assert_eq!(s.currency, "USD");
        assert_eq!(s.status, "authorized");
        assert_eq!(s.expires_at.as_deref(), Some("2099-01-01T00:00:00Z"));
    }
}
