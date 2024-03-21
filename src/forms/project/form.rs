use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use actix_web::Error;
use actix_web::web::Bytes;
use crate::models;
use crate::forms;
use crate::helpers::JsonResponse;
use std::str;


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct ProjectForm {
    pub custom: forms::project::Custom
}

impl TryFrom<&models::Project> for ProjectForm {
    type Error = String;

    fn try_from(project: &models::Project) -> Result<Self, Self::Error> {
        serde_json::from_value::<ProjectForm>(project.body.clone()).map_err(|err| format!("{:?}", err))
    }
}

impl ProjectForm {
    pub async fn is_readable_docker_image(&self) -> Result<bool, String> {
        let mut is_active = true;
        for app in &self.custom.web {
            if !app.app.docker_image.is_active().await? {
                is_active = false;
                break;
            }
        }

        if let Some(service) = &self.custom.service {
            for app in service {
                if !app.app.docker_image.is_active().await? {
                    is_active = false;
                    break;
                }
            }
        }

        if let Some(features) = &self.custom.feature {
            for app in features {
                if !app.app.docker_image.is_active().await? {
                    is_active = false;
                    break;
                }
            }
        }
        Ok(is_active)
    }
}

pub(crate) async fn body_into_form(body: Bytes) -> actix_web::Result<forms::project::ProjectForm, Error> {
    let body_bytes = actix_web::body::to_bytes(body).await.unwrap();
    let body_str = str::from_utf8(&body_bytes)
        .map_err(|err| JsonResponse::<forms::project::ProjectForm>::build().internal_server_error(err.to_string()))?;
    let deserializer = &mut serde_json::Deserializer::from_str(body_str);
    serde_path_to_error::deserialize(deserializer)
        .map_err(|err| {
            let msg = format!("{}:{:?}", err.path().to_string(), err);
            JsonResponse::<forms::project::ProjectForm>::build().bad_request(msg)
        })
        .and_then(|form: forms::project::ProjectForm| {
            if !form.validate().is_ok() {
                let errors = form.validate().unwrap_err().to_string();
                let err_msg = format!("Invalid data received {:?}", &errors);
                tracing::debug!(err_msg);

                return Err(JsonResponse::<models::Project>::build().form_error(errors));
            }

            Ok(form)
        })
}
