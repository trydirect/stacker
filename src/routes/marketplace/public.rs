use crate::db;
use crate::helpers::JsonResponse;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;

#[tracing::instrument(name = "List approved templates (public)")]
#[get("")]
pub async fn list_handler(
    query: web::Query<TemplateListQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let category = query.category.as_deref();
    let tag = query.tag.as_deref();
    let sort = query.sort.as_deref();

    db::marketplace::list_approved(pg_pool.get_ref(), category, tag, sort)
        .await
        .map_err(|err| JsonResponse::<Vec<crate::models::StackTemplate>>::build().internal_server_error(err))
        .map(|templates| JsonResponse::build().set_list(templates).ok("OK"))
}

#[derive(Debug, serde::Deserialize)]
pub struct TemplateListQuery {
    pub category: Option<String>,
    pub tag: Option<String>,
    pub sort: Option<String>, // recent|popular|rating
}

#[tracing::instrument(name = "Get template by slug (public)")]
#[get("/{slug}")]
pub async fn detail_handler(
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let slug = path.into_inner().0;

    match db::marketplace::get_by_slug_with_latest(pg_pool.get_ref(), &slug).await {
        Ok((template, version)) => {
            let mut payload = serde_json::json!({
                "template": template,
            });
            if let Some(ver) = version {
                payload["latest_version"] = serde_json::to_value(ver).unwrap();
            }
            Ok(JsonResponse::build().set_item(Some(payload)).ok("OK"))
        }
        Err(err) => Err(JsonResponse::<serde_json::Value>::build().not_found(err)),
    }
}
