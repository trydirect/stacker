use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{patch, web, Responder, Result};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct ProtectionForm {
    pub is_protected: bool,
    pub confirmation_name: Option<String>,
}

#[tracing::instrument(name = "Toggle project protection.", skip_all)]
#[patch("/{id}/protection")]
pub async fn toggle(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    web::Json(form): web::Json<ProtectionForm>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (id,) = path.into_inner();

    let project = db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .and_then(|project| match project {
            Some(project) if project.user_id != user.id => {
                Err(JsonResponse::<models::Project>::build().not_found(""))
            }
            Some(project) => Ok(project),
            None => Err(JsonResponse::<models::Project>::build().not_found("")),
        })?;

    if !form.is_protected {
        let confirmation = form.confirmation_name.as_deref().unwrap_or("");
        if confirmation != project.name {
            return Err(
                JsonResponse::<models::Project>::build().bad_request(format!(
                    "Project name '{}' does not match. Enter the exact project name to confirm.",
                    confirmation
                )),
            );
        }
    }

    db::project::set_protected(pg_pool.get_ref(), project.id, &user.id, form.is_protected)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .and_then(|updated| {
            if !updated {
                return Err(JsonResponse::<models::Project>::build().not_found("Project not found"));
            }
            let mut p = project;
            p.is_protected = form.is_protected;
            Ok(JsonResponse::<models::Project>::build()
                .set_item(p)
                .ok("success"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protection_form_enable() {
        let form: ProtectionForm = serde_json::from_str(r#"{"is_protected": true}"#).unwrap();
        assert!(form.is_protected);
        assert!(form.confirmation_name.is_none());
    }

    #[test]
    fn test_protection_form_disable_with_name() {
        let form: ProtectionForm =
            serde_json::from_str(r#"{"is_protected": false, "confirmation_name": "my-project"}"#)
                .unwrap();
        assert!(!form.is_protected);
        assert_eq!(form.confirmation_name.as_deref(), Some("my-project"));
    }

    #[test]
    fn test_protection_form_disable_without_name() {
        let form: ProtectionForm = serde_json::from_str(r#"{"is_protected": false}"#).unwrap();
        assert!(!form.is_protected);
        assert!(form.confirmation_name.is_none());
    }

    #[test]
    fn test_protection_form_missing_is_protected_fails() {
        let result = serde_json::from_str::<ProtectionForm>(r#"{"confirmation_name": "x"}"#);
        assert!(result.is_err(), "is_protected should be required");
    }
}
