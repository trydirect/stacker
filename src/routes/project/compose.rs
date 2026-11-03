use crate::db;
use crate::helpers::project::builder::DcBuilder;
use crate::helpers::JsonResponse;
use crate::models;
use crate::routes::project::access::{self, Need};
use actix_web::{get, web, web::Data, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;

#[tracing::instrument(name = "User's generate docker-compose.", skip_all)]
#[get("/{id}/compose")]
pub async fn add(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    let project = db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .and_then(|project| match project {
            Some(project) => Ok(project),
            None => Err(JsonResponse::<models::Project>::build().not_found("not found")),
        })?;
    access::check(
        pg_pool.get_ref(),
        &project,
        &user.id,
        Need::View,
        "not found",
    )
    .await?;

    DcBuilder::new(project)
        .build()
        .map_err(|err| JsonResponse::<models::Project>::build().bad_request(err))
        .map(|fc| JsonResponse::build().set_id(id).set_item(fc).ok("Success"))
}

#[tracing::instrument(name = "Generate docker-compose. Admin", skip_all)]
#[get("/{id}/compose")]
pub async fn admin(
    _user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    //  Admin function for generating compose file for specified user
    let id = path.0;
    let project = db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .and_then(|project| match project {
            Some(project) => Ok(project),
            None => Err(JsonResponse::<models::Project>::build().not_found("not found")),
        })?;

    DcBuilder::new(project)
        .build()
        .map_err(|err| JsonResponse::<models::Project>::build().bad_request(err))
        .map(|fc| JsonResponse::build().set_id(id).set_item(fc).ok("Success"))
}
