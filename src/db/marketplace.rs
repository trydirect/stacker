use crate::models::{StackTemplate, StackTemplateVersion};
use sqlx::PgPool;
use tracing::Instrument;

pub async fn list_approved(pool: &PgPool, category: Option<&str>, tag: Option<&str>, sort: Option<&str>) -> Result<Vec<StackTemplate>, String> {
    let mut base = String::from(
        r#"SELECT 
            id,
            creator_user_id,
            creator_name,
            name,
            slug,
            short_description,
            long_description,
            category_id,
            tags,
            tech_stack,
            status,
            plan_type,
            price,
            currency,
            is_configurable,
            view_count,
            deploy_count,
            average_rating,
            created_at,
            updated_at,
            approved_at
        FROM stack_template
        WHERE status = 'approved'"#,
    );

    if category.is_some() {
        base.push_str(" AND category_id = (SELECT id FROM stack_category WHERE name = $1)");
    }
    if tag.is_some() {
        base.push_str(r" AND tags \? $2");
    }

    match sort.unwrap_or("recent") {
        "popular" => base.push_str(" ORDER BY deploy_count DESC, view_count DESC"),
        "rating" => base.push_str(" ORDER BY average_rating DESC NULLS LAST"),
        _ => base.push_str(" ORDER BY approved_at DESC NULLS LAST, created_at DESC"),
    }

    let query_span = tracing::info_span!("marketplace_list_approved");

    let res = if category.is_some() && tag.is_some() {
        sqlx::query_as::<_, StackTemplate>(&base)
            .bind(category.unwrap())
            .bind(tag.unwrap())
            .fetch_all(pool)
            .instrument(query_span)
            .await
    } else if category.is_some() {
        sqlx::query_as::<_, StackTemplate>(&base)
            .bind(category.unwrap())
            .fetch_all(pool)
            .instrument(query_span)
            .await
    } else if tag.is_some() {
        sqlx::query_as::<_, StackTemplate>(&base)
            .bind(tag.unwrap())
            .fetch_all(pool)
            .instrument(query_span)
            .await
    } else {
        sqlx::query_as::<_, StackTemplate>(&base)
            .fetch_all(pool)
            .instrument(query_span)
            .await
    };

    res.map_err(|e| {
        tracing::error!("list_approved error: {:?}", e);
        "Internal Server Error".to_string()
    })
}

pub async fn get_by_slug_with_latest(pool: &PgPool, slug: &str) -> Result<(StackTemplate, Option<StackTemplateVersion>), String> {
    let query_span = tracing::info_span!("marketplace_get_by_slug_with_latest", slug = %slug);

    let template = sqlx::query_as!(
        StackTemplate,
        r#"SELECT 
            id,
            creator_user_id,
            creator_name,
            name,
            slug,
            short_description,
            long_description,
            category_id,
            tags,
            tech_stack,
            status,
            plan_type,
            price,
            currency,
            is_configurable,
            view_count,
            deploy_count,
            average_rating,
            created_at,
            updated_at,
            approved_at
        FROM stack_template WHERE slug = $1 AND status = 'approved'"#,
        slug
    )
    .fetch_one(pool)
    .instrument(query_span.clone())
    .await
    .map_err(|e| {
        tracing::error!("get_by_slug template error: {:?}", e);
        "Not Found".to_string()
    })?;

    let version = sqlx::query_as!(
        StackTemplateVersion,
        r#"SELECT 
            id,
            template_id,
            version,
            stack_definition,
            definition_format,
            changelog,
            is_latest,
            created_at
        FROM stack_template_version WHERE template_id = $1 AND is_latest = true LIMIT 1"#,
        template.id
    )
    .fetch_optional(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("get_by_slug version error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    Ok((template, version))
}

pub async fn create_draft(
    pool: &PgPool,
    creator_user_id: &str,
    creator_name: Option<&str>,
    name: &str,
    slug: &str,
    short_description: Option<&str>,
    long_description: Option<&str>,
    category_id: Option<i32>,
    tags: serde_json::Value,
    tech_stack: serde_json::Value,
) -> Result<StackTemplate, String> {
    let query_span = tracing::info_span!("marketplace_create_draft", slug = %slug);

    let rec = sqlx::query_as!(
        StackTemplate,
        r#"INSERT INTO stack_template (
            creator_user_id, creator_name, name, slug,
            short_description, long_description, category_id,
            tags, tech_stack, status
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'draft')
        RETURNING 
            id,
            creator_user_id,
            creator_name,
            name,
            slug,
            short_description,
            long_description,
            category_id,
            tags,
            tech_stack,
            status,
            plan_type,
            price,
            currency,
            is_configurable,
            view_count,
            deploy_count,
            average_rating,
            created_at,
            updated_at,
            approved_at
        "#,
        creator_user_id,
        creator_name,
        name,
        slug,
        short_description,
        long_description,
        category_id,
        tags,
        tech_stack
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("create_draft error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    Ok(rec)
}

pub async fn set_latest_version(pool: &PgPool, template_id: &uuid::Uuid, version: &str, stack_definition: serde_json::Value, definition_format: Option<&str>, changelog: Option<&str>) -> Result<StackTemplateVersion, String> {
    let query_span = tracing::info_span!("marketplace_set_latest_version", template_id = %template_id);

    // Clear previous latest
    sqlx::query!(
        r#"UPDATE stack_template_version SET is_latest = false WHERE template_id = $1 AND is_latest = true"#,
        template_id
    )
    .execute(pool)
    .instrument(query_span.clone())
    .await
    .map_err(|e| {
        tracing::error!("clear_latest error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    let rec = sqlx::query_as!(
        StackTemplateVersion,
        r#"INSERT INTO stack_template_version (
            template_id, version, stack_definition, definition_format, changelog, is_latest
        ) VALUES ($1,$2,$3,$4,$5,true)
        RETURNING id, template_id, version, stack_definition, definition_format, changelog, is_latest, created_at"#,
        template_id,
        version,
        stack_definition,
        definition_format,
        changelog
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("set_latest_version error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    Ok(rec)
}

pub async fn update_metadata(pool: &PgPool, template_id: &uuid::Uuid, name: Option<&str>, short_description: Option<&str>, long_description: Option<&str>, category_id: Option<i32>, tags: Option<serde_json::Value>, tech_stack: Option<serde_json::Value>, plan_type: Option<&str>, price: Option<f64>, currency: Option<&str>) -> Result<bool, String> {
    let query_span = tracing::info_span!("marketplace_update_metadata", template_id = %template_id);

    // Update only allowed statuses
    let status = sqlx::query_scalar!(
        r#"SELECT status FROM stack_template WHERE id = $1::uuid"#,
        template_id
    )
    .fetch_one(pool)
    .instrument(query_span.clone())
    .await
    .map_err(|e| {
        tracing::error!("get status error: {:?}", e);
        "Not Found".to_string()
    })?;

    if status != "draft" && status != "rejected" {
        return Err("Template not editable in current status".to_string());
    }

    let res = sqlx::query!(
        r#"UPDATE stack_template SET 
            name = COALESCE($2, name),
            short_description = COALESCE($3, short_description),
            long_description = COALESCE($4, long_description),
            category_id = COALESCE($5, category_id),
            tags = COALESCE($6, tags),
            tech_stack = COALESCE($7, tech_stack),
            plan_type = COALESCE($8, plan_type),
            price = COALESCE($9, price),
            currency = COALESCE($10, currency)
        WHERE id = $1::uuid"#,
        template_id,
        name,
        short_description,
        long_description,
        category_id,
        tags,
        tech_stack,
        plan_type,
        price,
        currency
    )
    .execute(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("update_metadata error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    Ok(res.rows_affected() > 0)
}

pub async fn submit_for_review(pool: &PgPool, template_id: &uuid::Uuid) -> Result<bool, String> {
    let query_span = tracing::info_span!("marketplace_submit_for_review", template_id = %template_id);

    let res = sqlx::query!(
        r#"UPDATE stack_template SET status = 'submitted' WHERE id = $1::uuid AND status IN ('draft','rejected')"#,
        template_id
    )
    .execute(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("submit_for_review error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    Ok(res.rows_affected() > 0)
}

pub async fn list_mine(pool: &PgPool, user_id: &str) -> Result<Vec<StackTemplate>, String> {
    let query_span = tracing::info_span!("marketplace_list_mine", user = %user_id);

    sqlx::query_as!(
        StackTemplate,
        r#"SELECT 
            id,
            creator_user_id,
            creator_name,
            name,
            slug,
            short_description,
            long_description,
            category_id,
            tags,
            tech_stack,
            status,
            plan_type,
            price,
            currency,
            is_configurable,
            view_count,
            deploy_count,
            average_rating,
            created_at,
            updated_at,
            approved_at
        FROM stack_template WHERE creator_user_id = $1 ORDER BY created_at DESC"#,
        user_id
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("list_mine error: {:?}", e);
        "Internal Server Error".to_string()
    })
}

pub async fn admin_list_submitted(pool: &PgPool) -> Result<Vec<StackTemplate>, String> {
    let query_span = tracing::info_span!("marketplace_admin_list_submitted");

    sqlx::query_as!(
        StackTemplate,
        r#"SELECT 
            id,
            creator_user_id,
            creator_name,
            name,
            slug,
            short_description,
            long_description,
            category_id,
            tags,
            tech_stack,
            status,
            plan_type,
            price,
            currency,
            is_configurable,
            view_count,
            deploy_count,
            average_rating,
            created_at,
            updated_at,
            approved_at
        FROM stack_template WHERE status = 'submitted' ORDER BY created_at ASC"#
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("admin_list_submitted error: {:?}", e);
        "Internal Server Error".to_string()
    })
}

pub async fn admin_decide(pool: &PgPool, template_id: &uuid::Uuid, reviewer_user_id: &str, decision: &str, review_reason: Option<&str>) -> Result<bool, String> {
    let query_span = tracing::info_span!("marketplace_admin_decide", template_id = %template_id, decision = %decision);

    let valid = ["approved", "rejected", "needs_changes"];
    if !valid.contains(&decision) {
        return Err("Invalid decision".to_string());
    }

    let mut tx = pool.begin().await.map_err(|e| {
        tracing::error!("tx begin error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    sqlx::query!(
        r#"INSERT INTO stack_template_review (template_id, reviewer_user_id, decision, review_reason, reviewed_at) VALUES ($1::uuid, $2, $3, $4, now())"#,
        template_id,
        reviewer_user_id,
        decision,
        review_reason
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("insert review error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    let status_sql = if decision == "approved" { "approved" } else if decision == "rejected" { "rejected" } else { "under_review" };
    let should_set_approved = decision == "approved";

    sqlx::query!(
        r#"UPDATE stack_template SET status = $2, approved_at = CASE WHEN $3 THEN now() ELSE approved_at END WHERE id = $1::uuid"#,
        template_id,
        status_sql,
        should_set_approved
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("update template status error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!("tx commit error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    Ok(true)
}
