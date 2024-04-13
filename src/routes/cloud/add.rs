use std::ops::Deref;
use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{post, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use chrono::Utc;
use serde_valid::Validate;
use tracing::Instrument;


#[tracing::instrument(name = "Add cloud.")]
#[post("")]
pub async fn add(
    user: web::ReqData<Arc<models::User>>,
    mut form: web::Json<forms::cloud::CloudForm>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {

    if !form.validate().is_ok() {
        let errors = form.validate().unwrap_err().to_string();
        let err_msg = format!("Invalid data received {:?}", &errors);
        tracing::debug!(err_msg);

        return Err(JsonResponse::<models::Project>::build().form_error(errors));
    }

    form.user_id = Some(user.id.clone());
    let cloud: models::Cloud = form.deref().into();

    db::cloud::insert(pg_pool.get_ref(), cloud)
        .await
        .map(|cloud| JsonResponse::build()
            .set_item(cloud)
            .ok("success"))
        .map_err(|_err| JsonResponse::<models::Cloud>::build()
            .internal_server_error("Failed to insert"))
}
