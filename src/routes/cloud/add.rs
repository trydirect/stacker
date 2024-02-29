use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{post, web, Responder, Result};
use sqlx::PgPool;
use tracing::Instrument;
use std::sync::Arc;
use serde_valid::Validate;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[tracing::instrument(name = "Add cloud.")]
#[post("")]
pub async fn add(
    user: web::ReqData<Arc<models::User>>,
    form: web::Json<forms::cloud::Cloud>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {

    if !form.validate().is_ok() {
        let errors = form.validate().unwrap_err().to_string();
        let err_msg = format!("Invalid data received {:?}", &errors);
        tracing::debug!(err_msg);

        return Err(JsonResponse::<models::Project>::build().form_error(errors));
    }

    let mut cloud: models::Cloud = form.into_inner().into();
    cloud.user_id = user.id.clone();

    db::cloud::insert(pg_pool.get_ref(), cloud)
        .await
        .map(|cloud| JsonResponse::build()
            .set_item(cloud)
            .ok("success"))
        .map_err(|_err| JsonResponse::<models::Cloud>::build()
            .internal_server_error("Failed to insert"))
}
