use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, post, put, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use uuid;

#[derive(Debug, serde::Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub slug: String,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub category_code: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub tech_stack: Option<serde_json::Value>,
    pub version: Option<String>,
    pub stack_definition: Option<serde_json::Value>,
    pub definition_format: Option<String>,
    /// Pricing: "free", "one_time", or "subscription"
    pub plan_type: Option<String>,
    /// Price amount (e.g. 9.99). Ignored when plan_type is "free"
    pub price: Option<f64>,
    /// ISO 4217 currency code, default "USD"
    pub currency: Option<String>,
}

#[tracing::instrument(name = "Create draft template")]
#[post("")]
pub async fn create_handler(
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
    body: web::Json<CreateTemplateRequest>,
) -> Result<impl Responder> {
    let req = body.into_inner();

    let tags = req.tags.unwrap_or(serde_json::json!([]));
    let tech_stack = req.tech_stack.unwrap_or(serde_json::json!({}));

    let creator_name = format!("{} {}", user.first_name, user.last_name);

    // Normalize pricing: plan_type "free" forces price to 0
    let billing_cycle = req.plan_type.unwrap_or_else(|| "free".to_string());
    let price = if billing_cycle == "free" { 0.0 } else { req.price.unwrap_or(0.0) };
    let currency = req.currency.unwrap_or_else(|| "USD".to_string());

    // Check if template with this slug already exists for this user
    let existing = db::marketplace::get_by_slug_and_user(pg_pool.get_ref(), &req.slug, &user.id)
        .await
        .ok();

    let template = if let Some(existing_template) = existing {
        // Update existing template
        tracing::info!("Updating existing template with slug: {}", req.slug);
        let updated = db::marketplace::update_metadata(
            pg_pool.get_ref(),
            &existing_template.id,
            Some(&req.name),
            req.short_description.as_deref(),
            req.long_description.as_deref(),
            req.category_code.as_deref(),
            Some(tags.clone()),
            Some(tech_stack.clone()),
            Some(price),
            Some(billing_cycle.as_str()),
            Some(currency.as_str()),
        )
        .await
        .map_err(|err| JsonResponse::<models::StackTemplate>::build().internal_server_error(err))?;

        if !updated {
            return Err(JsonResponse::<models::StackTemplate>::build()
                .internal_server_error("Failed to update template"));
        }

        // Fetch updated template
        db::marketplace::get_by_id(pg_pool.get_ref(), existing_template.id)
            .await
            .map_err(|err| {
                JsonResponse::<models::StackTemplate>::build().internal_server_error(err)
            })?
            .ok_or_else(|| {
                JsonResponse::<models::StackTemplate>::build()
                    .not_found("Template not found after update")
            })?
    } else {
        // Create new template
        db::marketplace::create_draft(
            pg_pool.get_ref(),
            &user.id,
            Some(&creator_name),
            &req.name,
            &req.slug,
            req.short_description.as_deref(),
            req.long_description.as_deref(),
            req.category_code.as_deref(),
            tags,
            tech_stack,
            price,
            &billing_cycle,
            &currency,
        )
        .await
        .map_err(|err| {
            // If error message indicates duplicate slug, return 409 Conflict
            if err.contains("already in use") {
                return JsonResponse::<models::StackTemplate>::build().conflict(err);
            }
            JsonResponse::<models::StackTemplate>::build().internal_server_error(err)
        })?
    };

    // Optional initial version
    if let Some(def) = req.stack_definition {
        let version = req.version.unwrap_or("1.0.0".to_string());
        let _ = db::marketplace::set_latest_version(
            pg_pool.get_ref(),
            &template.id,
            &version,
            def,
            req.definition_format.as_deref(),
            None,
        )
        .await;
    }

    Ok(JsonResponse::build()
        .set_item(Some(template))
        .created("Created"))
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub category_code: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub tech_stack: Option<serde_json::Value>,
    pub plan_type: Option<String>,
    pub price: Option<f64>,
    pub currency: Option<String>,
}

#[tracing::instrument(name = "Update template metadata")]
#[put("/{id}")]
pub async fn update_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
    body: web::Json<UpdateTemplateRequest>,
) -> Result<web::Json<crate::helpers::json::JsonResponse<serde_json::Value>>> {
    let id = uuid::Uuid::parse_str(&path.into_inner().0)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;

    // Ownership check
    let owner_id: String = sqlx::query_scalar!(
        r#"SELECT creator_user_id FROM stack_template WHERE id = $1"#,
        id
    )
    .fetch_one(pg_pool.get_ref())
    .await
    .map_err(|_| JsonResponse::<serde_json::Value>::build().not_found("Not Found"))?;

    if owner_id != user.id {
        return Err(JsonResponse::<serde_json::Value>::build().forbidden("Forbidden"));
    }

    let req = body.into_inner();

    let updated = db::marketplace::update_metadata(
        pg_pool.get_ref(),
        &id,
        req.name.as_deref(),
        req.short_description.as_deref(),
        req.long_description.as_deref(),
        req.category_code.as_deref(),
        req.tags,
        req.tech_stack,
        req.price,
        req.plan_type.as_deref(),
        req.currency.as_deref(),
    )
    .await
    .map_err(|err| JsonResponse::<serde_json::Value>::build().bad_request(err))?;

    if updated {
        Ok(JsonResponse::<serde_json::Value>::build().ok("Updated"))
    } else {
        Err(JsonResponse::<serde_json::Value>::build().not_found("Not Found"))
    }
}

#[tracing::instrument(name = "Submit template for review")]
#[post("/{id}/submit")]
pub async fn submit_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<web::Json<crate::helpers::json::JsonResponse<serde_json::Value>>> {
    let id = uuid::Uuid::parse_str(&path.into_inner().0)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;

    // Ownership check
    let owner_id: String = sqlx::query_scalar!(
        r#"SELECT creator_user_id FROM stack_template WHERE id = $1"#,
        id
    )
    .fetch_one(pg_pool.get_ref())
    .await
    .map_err(|_| JsonResponse::<serde_json::Value>::build().not_found("Not Found"))?;

    if owner_id != user.id {
        return Err(JsonResponse::<serde_json::Value>::build().forbidden("Forbidden"));
    }

    let submitted = db::marketplace::submit_for_review(pg_pool.get_ref(), &id)
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?;

    if submitted {
        Ok(JsonResponse::<serde_json::Value>::build().ok("Submitted"))
    } else {
        Err(JsonResponse::<serde_json::Value>::build().bad_request("Invalid status"))
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ResubmitRequest {
    pub version: String,
    pub stack_definition: serde_json::Value,
    pub definition_format: Option<String>,
    pub changelog: Option<String>,
}

#[tracing::instrument(name = "Resubmit template with new version")]
#[post("/{id}/resubmit")]
pub async fn resubmit_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
    body: web::Json<ResubmitRequest>,
) -> Result<web::Json<crate::helpers::json::JsonResponse<serde_json::Value>>> {
    let id = uuid::Uuid::parse_str(&path.into_inner().0)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;

    // Ownership check
    let owner_id: String = sqlx::query_scalar!(
        r#"SELECT creator_user_id FROM stack_template WHERE id = $1"#,
        id
    )
    .fetch_one(pg_pool.get_ref())
    .await
    .map_err(|_| JsonResponse::<serde_json::Value>::build().not_found("Not Found"))?;

    if owner_id != user.id {
        return Err(JsonResponse::<serde_json::Value>::build().forbidden("Forbidden"));
    }

    let req = body.into_inner();

    let version = db::marketplace::resubmit_with_new_version(
        pg_pool.get_ref(),
        &id,
        &req.version,
        req.stack_definition,
        req.definition_format.as_deref(),
        req.changelog.as_deref(),
    )
    .await
    .map_err(|err| JsonResponse::<serde_json::Value>::build().bad_request(err))?;

    let result = serde_json::json!({
        "template_id": id,
        "version": version,
        "status": "submitted"
    });

    Ok(JsonResponse::<serde_json::Value>::build()
        .set_item(result)
        .ok("Resubmitted for review"))
}

#[tracing::instrument(name = "List my templates")]
#[get("/mine")]
pub async fn mine_handler(
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::marketplace::list_mine(pg_pool.get_ref(), &user.id)
        .await
        .map_err(|err| {
            JsonResponse::<Vec<models::StackTemplate>>::build().internal_server_error(err)
        })
        .map(|templates| JsonResponse::build().set_list(templates).ok("OK"))
}
