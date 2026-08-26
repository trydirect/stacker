use std::collections::HashMap;
use std::sync::Arc;

use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;

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

#[derive(Debug, sqlx::FromRow)]
struct TemplatePricing {
    slug: String,
    price: Option<f64>,
    billing_cycle: Option<String>,
    required_plan_name: Option<String>,
    creator_name: Option<String>,
}

async fn enrich_items_with_pricing(pg_pool: &PgPool, items: &mut [serde_json::Value]) {
    let slugs: Vec<String> = items
        .iter()
        .filter_map(|item| {
            item.get("code")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    if slugs.is_empty() {
        return;
    }

    let pricing = sqlx::query_as::<_, TemplatePricing>(
        r#"
        SELECT slug, price, billing_cycle, required_plan_name, creator_name
        FROM stack_template
        WHERE slug = ANY($1) AND status = 'approved'
        "#,
    )
    .bind(&slugs)
    .fetch_all(pg_pool)
    .await;

    let pricing_map: HashMap<String, TemplatePricing> = match pricing {
        Ok(rows) => rows.into_iter().map(|r| (r.slug.clone(), r)).collect(),
        Err(e) => {
            tracing::warn!("Failed to fetch template pricing: {}", e);
            return;
        }
    };

    for item in items.iter_mut() {
        let slug = match item.get("code").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        if let Some(p) = pricing_map.get(slug) {
            if let Some(price) = p.price {
                item["price"] = serde_json::json!(price);
            }
            if let Some(ref cycle) = p.billing_cycle {
                item["billing_cycle"] = serde_json::json!(cycle);
            }
            if let Some(ref plan) = p.required_plan_name {
                item["required_plan_name"] = serde_json::json!(plan);
            }
            if let Some(ref creator) = p.creator_name {
                item["creator_name"] = serde_json::json!(creator);
            }
        }
    }
}

#[tracing::instrument(name = "Search marketplace applications catalog", skip_all)]
#[get("/applications")]
pub async fn applications_search_handler(
    query: web::Query<ApplicationSearchQuery>,
    user: web::ReqData<Arc<models::User>>,
    user_service: web::Data<Arc<dyn UserServiceConnector>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let token = user.access_token.as_deref().ok_or_else(|| {
        JsonResponse::<serde_json::Value>::build()
            .forbidden("User token is required to search applications")
    })?;

    let mut items = user_service
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

    enrich_items_with_pricing(pg_pool.get_ref(), &mut items).await;

    Ok(JsonResponse::build().set_list(items).ok("OK"))
}
