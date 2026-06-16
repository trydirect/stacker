use std::sync::Arc;

use actix_web::{post, web, Responder, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use serde_valid::Validate;
use sqlx::PgPool;

use crate::configuration::Settings;
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
}

#[derive(Debug, Serialize)]
pub struct InstallTemplateResponse {
    pub project: models::Project,
    pub template: serde_json::Value,
    pub latest_version: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<i32>,
}

fn map_access_error(err: services::MarketplaceAccessError) -> actix_web::Error {
    match err {
        services::MarketplaceAccessError::ValidationFailed(reason) => {
            tracing::error!("Marketplace access validation failed: {}", reason);
            JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Failed to validate marketplace access")
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
    let mut form: ProjectForm = serde_json::from_value(latest_version.stack_definition.clone())
        .map_err(|err| {
            JsonResponse::<serde_json::Value>::build().bad_request(format!(
                "Template '{}' cannot be installed because its stack definition is invalid: {}",
                template.slug, err
            ))
        })?;

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

    form.validate()
        .map_err(|err| JsonResponse::<serde_json::Value>::build().bad_request(err.to_string()))?;

    Ok(form)
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

fn apply_install_inputs_to_deploy(
    deploy_form: &mut forms::project::Deploy,
    install_inputs: &Map<String, Value>,
) {
    if install_inputs.is_empty() {
        return;
    }
    let vars = deploy_form.stack.vars.get_or_insert_with(Vec::new);
    for (key, value) in install_inputs {
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

    let form = build_project_form(&template, &latest_version, request.name.as_deref())?;
    let mut request_json = serde_json::to_value(&form).map_err(|err| {
        tracing::error!("Failed to serialize marketplace project form: {:?}", err);
        JsonResponse::<serde_json::Value>::build().internal_server_error("Internal Server Error")
    })?;
    let install_inputs = normalized_install_inputs(&request.install_inputs);
    attach_install_inputs(&mut request_json, &install_inputs);

    let mut project = insert_project_from_form(pg_pool, user, &form, request_json).await?;
    project.source_template_id = Some(template.id);
    project.template_version = Some(latest_version.version.clone());
    project = db::project::update(pg_pool, project).await.map_err(|err| {
        tracing::error!("Failed to update installed template metadata: {}", err);
        JsonResponse::<serde_json::Value>::build().internal_server_error("Internal Server Error")
    })?;

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

    Ok(InstallTemplateResponse {
        project,
        template: serde_json::to_value(template).unwrap_or_else(|_| serde_json::json!({})),
        latest_version: serde_json::to_value(latest_version)
            .unwrap_or_else(|_| serde_json::json!({})),
        deployment_id,
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
    let token = user.access_token.as_deref().ok_or_else(|| {
        JsonResponse::<serde_json::Value>::build()
            .forbidden("User token is required to install catalog applications")
    })?;

    let applications = user_service
        .search_marketplace_templates(token, Some(slug), None, Some(true), Some(1), Some(10))
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
    let application = applications
        .into_iter()
        .find(|application| {
            application
                .get("code")
                .and_then(|value| value.as_str())
                .map(|code| code.to_ascii_lowercase() == slug_lc)
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            JsonResponse::<serde_json::Value>::build().not_found(format!(
                "Template or catalog application '{}' was not found",
                slug
            ))
        })?;

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
        Err(_) => {
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
    use super::catalog_application_project_form;
    use crate::{forms::project::Payload, models};
    use serde_json::json;

    #[test]
    fn catalog_application_project_form_preserves_catalog_context() {
        let application = json!({
            "code": "dify",
            "name": "Dify",
            "description": "Dify AI application platform",
            "categories": ["AI"],
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
}
