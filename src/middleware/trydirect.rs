use crate::configuration::Settings;
use futures::future::{FutureExt};
use crate::forms::user::UserForm;
use actix_web::dev::ServiceRequest;
use actix_web::error::{ErrorInternalServerError, ErrorUnauthorized};
use actix_web::web::{self};
use actix_web::Error;
use actix_web::HttpMessage;
use actix_web_httpauth::extractors::bearer::BearerAuth;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use crate::helpers::JsonResponse;

use crate::models::user::User; //todo

#[tracing::instrument(name = "Trydirect bearer guard.")]
pub async fn bearer_guard(
    req: ServiceRequest,
    credentials: BearerAuth,
    ) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    async {
        let settings = req.app_data::<web::Data<Settings>>().unwrap();
        let client = reqwest::Client::new();
        let resp = client
            .get(&settings.auth_url)
            .bearer_auth(credentials.token())
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .send()
            .await;

        let resp = match resp {
            Ok(resp) if resp.status().is_success() => resp,
            Ok(resp) => {
                tracing::error!("Authentication service returned no success {:?}", resp);
                return Err(("401 Unauthorized".to_string(), req));
            }
            Err(err) => {
                tracing::error!("error from reqwest {:?}", err);
                return Err((err.to_string(), req));
            }
        };

        let user_form: UserForm = match resp.json().await {
            Ok(user) => {
                tracing::info!("unpacked user {user:?}");
                user
            }
            Err(err) => {
                tracing::error!("can't parse the response body {:?}", err);
                return Err(("can't parse the response body".to_string(), req));
            }
        };

        let user: User = match user_form.try_into() {
            Ok(user) => user,
            Err(err) => {
                tracing::error!("Could not create User from form data: {:?}", err);
                return Err(("Could not create User from form data".to_string(), req));
            }
        };
        let existent_user = req.extensions_mut().insert(user);
        if existent_user.is_some() {
            tracing::error!("already logged {existent_user:?}");
            return Err(("user already logged".to_string(), req));
        }

        Ok(req)
    }.await
    .map_err(|(err, req)| {
        tracing::error!("Authentication service returned no success {:?}", err);
        (ErrorUnauthorized(JsonResponse::<i32>::build().set_msg(err).to_string()), req) //todo
                                                                                        //default
                                                                                        //type for
                                                                                        //JsonResponse
    })
}
