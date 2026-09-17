//! Developer project synchronization API.
//!
//! This endpoint persists the declarative project configuration without
//! creating a deployment, contacting a target server, or invoking the Install
//! Service. Runtime files are still uploaded by the normal deploy flow.

use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use crate::project_app;
use actix_web::{put, web, Responder, Result};
use serde_json::Value;
use serde_valid::Validate;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

/// Remove secret-shaped values before project metadata is persisted. Existing
/// values are restored from project_app below; new secret values must use the
/// Vault-backed service-secret API instead of PostgreSQL project metadata.
fn strip_sensitive_environment(request: &mut Value) -> usize {
    let Some(custom) = request.get_mut("custom").and_then(Value::as_object_mut) else {
        return 0;
    };

    let mut removed = 0;
    for group in ["web", "service", "feature"] {
        let Some(apps) = custom.get_mut(group).and_then(Value::as_array_mut) else {
            continue;
        };
        for app in apps {
            let Some(environment) = app.get_mut("environment").and_then(Value::as_array_mut) else {
                continue;
            };
            let before = environment.len();
            environment.retain(|entry| {
                !entry
                    .get("key")
                    .and_then(Value::as_str)
                    .is_some_and(crate::helpers::redact::is_sensitive_env_key)
            });
            removed += before - environment.len();
        }
    }
    removed
}

fn sensitive_values_by_app(
    apps: &[models::ProjectApp],
) -> HashMap<String, serde_json::Map<String, Value>> {
    apps.iter()
        .filter_map(|app| {
            let values = app.environment.as_ref()?.as_object()?;
            let sensitive = values
                .iter()
                .filter(|(key, _)| crate::helpers::redact::is_sensitive_env_key(key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<serde_json::Map<_, _>>();
            (!sensitive.is_empty()).then(|| (app.code.clone(), sensitive))
        })
        .collect()
}

fn validate_marketplace_metadata(request: &Value) -> Result<(), String> {
    let Some(custom) = request.get("custom").and_then(Value::as_object) else {
        return Ok(());
    };

    if let Some(assets) = custom.get("marketplace_assets") {
        let assets = assets
            .as_array()
            .ok_or_else(|| "custom.marketplace_assets must be an array".to_string())?;
        for (index, asset) in assets.iter().enumerate() {
            let object = asset
                .as_object()
                .ok_or_else(|| format!("custom.marketplace_assets[{index}] must be an object"))?;
            for field in ["filename", "sha256"] {
                if object
                    .get(field)
                    .and_then(Value::as_str)
                    .is_none_or(|value| value.trim().is_empty())
                {
                    return Err(format!(
                        "custom.marketplace_assets[{index}].{field} is required"
                    ));
                }
            }
            if object
                .get("size")
                .and_then(Value::as_i64)
                .is_none_or(|size| size <= 0)
            {
                return Err(format!(
                    "custom.marketplace_assets[{index}].size must be positive"
                ));
            }
        }
    }

    if let Some(seed_jobs) = custom.get("marketplace_seed_jobs") {
        let seed_jobs = seed_jobs
            .as_array()
            .ok_or_else(|| "custom.marketplace_seed_jobs must be an array".to_string())?;
        let mut names = std::collections::HashSet::new();
        for (index, job) in seed_jobs.iter().enumerate() {
            let name = job
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| format!("custom.marketplace_seed_jobs[{index}].name is required"))?;
            if !names.insert(name.to_string()) {
                return Err(format!(
                    "custom.marketplace_seed_jobs contains duplicate name '{name}'"
                ));
            }
        }
    }

    Ok(())
}

async fn restore_existing_sensitive_values(
    pool: &PgPool,
    project_id: i32,
    values_by_app: HashMap<String, serde_json::Map<String, Value>>,
) -> Result<(), String> {
    for mut app in db::project_app::fetch_by_project(pool, project_id).await? {
        let Some(sensitive) = values_by_app.get(&app.code) else {
            continue;
        };
        let mut environment = app
            .environment
            .take()
            .unwrap_or_else(|| serde_json::json!({}));
        let Some(object) = environment.as_object_mut() else {
            continue;
        };
        object.extend(sensitive.clone());
        app.environment = Some(environment);
        db::project_app::update(pool, &app).await?;
    }
    Ok(())
}

/// Persist the local project declaration in Stacker without deploying it.
#[tracing::instrument(name = "Sync project configuration", skip_all)]
#[put("/{id}/sync")]
pub async fn item(
    path: web::Path<(i32,)>,
    web::Json(request_json): web::Json<Value>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    let mut project = db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(JsonResponse::internal_server_error)
        .and_then(|project| match project {
            Some(project) if project.user_id != user.id => {
                Err(JsonResponse::not_found("Project not found"))
            }
            Some(project) => Ok(project),
            None => Err(JsonResponse::not_found("Project not found")),
        })?;

    let existing_apps = db::project_app::fetch_by_project(pg_pool.get_ref(), id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    let existing_sensitive = sensitive_values_by_app(&existing_apps);
    let mut request_json = request_json;
    let secrets_omitted = strip_sensitive_environment(&mut request_json);
    validate_marketplace_metadata(&request_json)
        .map_err(|error| JsonResponse::bad_request(error))?;

    let form: crate::forms::project::ProjectForm = serde_json::from_value(request_json.clone())
        .map_err(|err| JsonResponse::bad_request(err.to_string()))?;

    form.validate()
        .map_err(|err| JsonResponse::bad_request(err.to_string()))?;

    match form.is_readable_docker_image().await {
        Ok(result) if result.readable => {}
        Ok(result) => {
            return Err(
                JsonResponse::<crate::forms::project::DockerImageReadResult>::build()
                    .set_item(result)
                    .bad_request("Can not access docker image"),
            );
        }
        Err(error) => return Err(JsonResponse::bad_request(error)),
    }

    project.name = form.custom.custom_stack_code.clone();
    project.metadata = serde_json::to_value(&form).map_err(|err| {
        JsonResponse::<models::Project>::build().internal_server_error(err.to_string())
    })?;
    project.request_json = request_json;

    let project = db::project::update(pg_pool.get_ref(), project)
        .await
        .map_err(|err| {
            tracing::error!("Failed to persist synced project: {:?}", err);
            JsonResponse::internal_server_error("")
        })?;

    project_app::sync_project_level_apps_from_form(pg_pool.get_ref(), project.id, &form)
        .await
        .map_err(|err| {
            tracing::error!(
                project_id = project.id,
                error = %err,
                "Failed to sync project apps"
            );
            JsonResponse::internal_server_error("")
        })?;

    restore_existing_sensitive_values(pg_pool.get_ref(), project.id, existing_sensitive)
        .await
        .map_err(|err| {
            tracing::error!(project_id = project.id, error = %err, "Failed to restore project secrets");
            JsonResponse::internal_server_error("")
        })?;

    Ok(JsonResponse::build()
        .set_item(serde_json::json!({
            "project_id": project.id,
            "status": "synced",
            "secrets_omitted": secrets_omitted,
            "deployment_created": false,
            "server_contacted": false,
            "containers_started": false,
        }))
        .ok("Project configuration synchronized"))
}

#[cfg(test)]
mod tests {
    use super::{strip_sensitive_environment, validate_marketplace_metadata};
    use serde_json::json;

    #[test]
    fn sync_identifies_secret_shaped_environment_keys() {
        assert!(crate::helpers::redact::is_sensitive_env_key(
            "AWS_SECRET_ACCESS_KEY"
        ));
        assert!(crate::helpers::redact::is_sensitive_env_key(
            "DATABASE_PASSWORD"
        ));
        assert!(!crate::helpers::redact::is_sensitive_env_key(
            "FLOCI_ENDPOINT"
        ));
        assert!(!crate::helpers::redact::is_sensitive_env_key("PUBLIC_URL"));
        assert!(!crate::helpers::redact::is_sensitive_env_key(
            "FLOCI_TLS_ENABLED"
        ));
    }

    #[test]
    fn sync_removes_secret_values_before_persistence() {
        let mut request = json!({
            "custom": {
                "web": [{
                    "environment": [
                        {"key": "FLOCI_ENDPOINT", "value": "http://floci:4566"},
                        {"key": "AWS_SECRET_ACCESS_KEY", "value": "hidden"}
                    ]
                }]
            }
        });

        assert_eq!(strip_sensitive_environment(&mut request), 1);
        assert_eq!(
            request["custom"]["web"][0]["environment"],
            json!([{"key": "FLOCI_ENDPOINT", "value": "http://floci:4566"}])
        );
    }

    #[test]
    fn sync_validates_asset_and_seed_job_metadata() {
        assert!(validate_marketplace_metadata(&json!({
            "custom": {
                "marketplace_assets": [{
                    "filename": "runtime.tgz",
                    "sha256": "abc",
                    "size": 12
                }],
                "marketplace_seed_jobs": [{"name": "seed-admin"}]
            }
        }))
        .is_ok());

        assert!(validate_marketplace_metadata(&json!({
            "custom": {"marketplace_assets": [{"filename": "runtime.tgz"}]}
        }))
        .is_err());

        assert!(validate_marketplace_metadata(&json!({
            "custom": {
                "marketplace_seed_jobs": [{"name": "seed-admin"}, {"name": "seed-admin"}]
            }
        }))
        .is_err());
    }
}
