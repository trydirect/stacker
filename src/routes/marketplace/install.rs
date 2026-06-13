use std::sync::Arc;

use actix_web::{post, web, Responder, Result};
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use sqlx::PgPool;

use crate::connectors::user_service::UserServiceConnector;
use crate::forms::project::ProjectForm;
use crate::helpers::JsonResponse;
use crate::{db, models, project_app, services};

#[derive(Debug, Deserialize)]
pub struct InstallTemplateRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallTemplateResponse {
    pub project: models::Project,
    pub template: models::StackTemplate,
    pub latest_version: models::StackTemplateVersion,
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

#[tracing::instrument(name = "Install marketplace template", skip_all)]
#[post("/{slug}/install")]
pub async fn install_handler(
    path: web::Path<(String,)>,
    request: web::Json<InstallTemplateRequest>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
    user_service: web::Data<Arc<dyn UserServiceConnector>>,
) -> Result<impl Responder> {
    let slug = path.into_inner().0;
    let (template, latest_version) =
        db::marketplace::get_by_slug_with_latest(pg_pool.get_ref(), &slug)
            .await
            .map_err(|err| JsonResponse::<serde_json::Value>::build().not_found(err))?;
    let latest_version = latest_version.ok_or_else(|| {
        JsonResponse::<serde_json::Value>::build().bad_request(format!(
            "Template '{}' has no installable version",
            template.slug
        ))
    })?;

    services::validate_marketplace_template_access(
        user_service.get_ref(),
        user.as_ref(),
        &template,
    )
    .await
    .map_err(map_access_error)?;

    let form = build_project_form(&template, &latest_version, request.name.as_deref())?;
    let project_name = form.custom.custom_stack_code.clone();
    let request_json = serde_json::to_value(&form).map_err(|err| {
        tracing::error!("Failed to serialize marketplace project form: {:?}", err);
        JsonResponse::<serde_json::Value>::build().internal_server_error("Internal Server Error")
    })?;
    let metadata = request_json.clone();

    let mut project = models::Project::new(user.id.clone(), project_name, metadata, request_json);
    project.source_template_id = Some(template.id);
    project.template_version = Some(latest_version.version.clone());

    let project = db::project::insert(pg_pool.get_ref(), project)
        .await
        .map_err(|err| {
            tracing::error!("Failed to create marketplace project: {}", err);
            JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Internal Server Error")
        })?;

    project_app::sync_project_level_apps_from_form(pg_pool.get_ref(), project.id, &form)
        .await
        .map_err(|err| {
            tracing::error!(
                "Failed to sync project-level apps for marketplace project {}: {}",
                project.id,
                err
            );
            JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Internal Server Error")
        })?;

    Ok(JsonResponse::build()
        .set_item(InstallTemplateResponse {
            project,
            template,
            latest_version,
        })
        .ok("Template installed"))
}
