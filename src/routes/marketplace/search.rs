use std::sync::Arc;

use actix_web::{get, web, Responder, Result};

use crate::connectors::user_service::UserServiceConnector;
use crate::helpers::JsonResponse;
use crate::models;

#[derive(Debug, serde::Deserialize)]
pub struct ApplicationSearchQuery {
    pub q: Option<String>,
    pub category: Option<String>,
    pub is_marketplace: Option<bool>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[tracing::instrument(name = "Search marketplace applications catalog", skip_all)]
#[get("/applications")]
pub async fn applications_search_handler(
    query: web::Query<ApplicationSearchQuery>,
    user: web::ReqData<Arc<models::User>>,
    user_service: web::Data<Arc<dyn UserServiceConnector>>,
) -> Result<impl Responder> {
    let token = user.access_token.as_deref().ok_or_else(|| {
        JsonResponse::<serde_json::Value>::build()
            .forbidden("User token is required to search applications")
    })?;

    let items = user_service
        .search_marketplace_templates(
            token,
            query.q.as_deref(),
            query.category.as_deref(),
            query.is_marketplace,
            query.page,
            query.limit,
        )
        .await
        .map_err(|err| {
            tracing::error!("Applications catalog search failed: {:?}", err);
            JsonResponse::<serde_json::Value>::build()
                .internal_server_error("Applications catalog search failed")
        })?;

    Ok(JsonResponse::build().set_list(items).ok("OK"))
}
