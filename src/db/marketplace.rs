use crate::models::{StackTemplate, StackTemplateVersion, StackCategory};
use sqlx::PgPool;
use tracing::Instrument;

pub async fn list_approved(pool: &PgPool, category: Option<&str>, tag: Option<&str>, sort: Option<&str>) -> Result<Vec<StackTemplate>, String> {
    let mut base = String::from(
        r#"SELECT 
            t.id,
            t.creator_user_id,
            t.creator_name,
            t.name,
            t.slug,
            t.short_description,
            t.long_description,
            c.name AS "category_code?",
            t.product_id,
            t.tags,
            t.tech_stack,
            t.status,
            t.is_configurable,
            t.view_count,
            t.deploy_count,
            t.required_plan_name,
            t.created_at,
            t.updated_at,
            t.approved_at
        FROM stack_template t
        LEFT JOIN stack_category c ON t.category_id = c.id
        WHERE t.status = 'approved'"#,
    );

    if category.is_some() {
        base.push_str(" AND c.name = $1");
    }
    if tag.is_some() {
        base.push_str(" AND t.tags ? $2");
    }

    match sort.unwrap_or("recent") {
        "popular" => base.push_str(" ORDER BY t.deploy_count DESC, t.view_count DESC"),
        "rating" => base.push_str(" ORDER BY (SELECT AVG(rate) FROM rating WHERE rating.product_id = t.product_id) DESC NULLS LAST"),
        _ => base.push_str(" ORDER BY t.approved_at DESC NULLS LAST, t.created_at DESC"),
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
            t.id,
            t.creator_user_id,
            t.creator_name,
            t.name,
            t.slug,
            t.short_description,
            t.long_description,
            c.name AS "category_code?",
            t.product_id,
            t.tags,
            t.tech_stack,
            t.status,
            t.is_configurable,
            t.view_count,
            t.deploy_count,
            t.required_plan_name,
            t.created_at,
            t.updated_at,
            t.approved_at
        FROM stack_template t
        LEFT JOIN stack_category c ON t.category_id = c.id
        WHERE t.slug = $1 AND t.status = 'approved'"#,
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

pub async fn get_by_id(pool: &PgPool, template_id: uuid::Uuid) -> Result<Option<StackTemplate>, String> {
    let query_span = tracing::info_span!("marketplace_get_by_id", id = %template_id);

    let template = sqlx::query_as!(
        StackTemplate,
        r#"SELECT 
            t.id,
            t.creator_user_id,
            t.creator_name,
            t.name,
            t.slug,
            t.short_description,
            t.long_description,
            c.name AS "category_code?",
            t.product_id,
            t.tags,
            t.tech_stack,
            t.status,
            t.is_configurable,
            t.view_count,
            t.deploy_count,
            t.created_at,
            t.updated_at,
            t.approved_at,
            t.required_plan_name
        FROM stack_template t
        LEFT JOIN stack_category c ON t.category_id = c.id
        WHERE t.id = $1"#,
        template_id
    )
    .fetch_optional(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("get_by_id error: {:?}", e);
        "Internal Server Error".to_string()
    })?;

    Ok(template)
}

pub async fn create_draft(
    pool: &PgPool,
    creator_user_id: &str,
    creator_name: Option<&str>,
    name: &str,
    slug: &str,
    short_description: Option<&str>,
    long_description: Option<&str>,
    category_code: Option<&str>,
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
        ) VALUES ($1,$2,$3,$4,$5,$6,(SELECT id FROM stack_category WHERE name = $7),$8,$9,'draft')
        RETURNING 
            id,
            creator_user_id,
            creator_name,
            name,
            slug,
            short_description,
            long_description,
            (SELECT name FROM stack_category WHERE id = category_id) AS "category_code?",
            product_id,
            tags,
            tech_stack,
            status,
            is_configurable,
            view_count,
            deploy_count,
            required_plan_name,
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
        category_code,
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

pub async fn update_metadata(pool: &PgPool, template_id: &uuid::Uuid, name: Option<&str>, short_description: Option<&str>, long_description: Option<&str>, category_code: Option<&str>, tags: Option<serde_json::Value>, tech_stack: Option<serde_json::Value>) -> Result<bool, String> {
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
            category_id = COALESCE((SELECT id FROM stack_category WHERE name = $5), category_id),
            tags = COALESCE($6, tags),
            tech_stack = COALESCE($7, tech_stack)
        WHERE id = $1::uuid"#,
        template_id,
        name,
        short_description,
        long_description,
        category_code,
        tags,
        tech_stack
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
            t.id,
            t.creator_user_id,
            t.creator_name,
            t.name,
            t.slug,
            t.short_description,
            t.long_description,
            c.name AS "category_code?",
            t.product_id,
            t.tags,
            t.tech_stack,
            t.status,
            t.is_configurable,
            t.view_count,
            t.deploy_count,
            t.required_plan_name,
            t.created_at,
            t.updated_at,
            t.approved_at
        FROM stack_template t
        LEFT JOIN stack_category c ON t.category_id = c.id
        WHERE t.creator_user_id = $1
        ORDER BY t.created_at DESC"#,
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
            t.id,
            t.creator_user_id,
            t.creator_name,
            t.name,
            t.slug,
            t.short_description,
            t.long_description,
            c.name AS "category_code?",
            t.product_id,
            t.tags,
            t.tech_stack,
            t.status,
            t.is_configurable,
            t.view_count,
            t.deploy_count,
            t.required_plan_name,
            t.created_at,
            t.updated_at,
            t.approved_at
        FROM stack_template t
        LEFT JOIN stack_category c ON t.category_id = c.id
        WHERE t.status = 'submitted'
        ORDER BY t.created_at ASC"#
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

/// Sync categories from User Service to local mirror
/// Upserts category data (id, name, title, metadata)
pub async fn sync_categories(
    pool: &PgPool,
    categories: Vec<crate::connectors::CategoryInfo>,
) -> Result<usize, String> {
    let query_span = tracing::info_span!("sync_categories", count = categories.len());
    let _enter = query_span.enter();

    if categories.is_empty() {
        tracing::info!("No categories to sync");
        return Ok(0);
    }

    let mut synced_count = 0;

    for category in categories {
        // Use INSERT ... ON CONFLICT DO UPDATE to upsert
        let result = sqlx::query(
            r#"
            INSERT INTO stack_category (id, name, title, metadata)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE
            SET name = EXCLUDED.name,
                title = EXCLUDED.title,
                metadata = EXCLUDED.metadata
            "#
        )
        .bind(category.id)
        .bind(&category.name)
        .bind(&category.title)
        .bind(serde_json::json!({"priority": category.priority}))
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to sync category {}: {:?}", category.name, e);
            format!("Failed to sync category: {}", e)
        })?;

        if result.rows_affected() > 0 {
            synced_count += 1;
        }
    }

    tracing::info!("Synced {} categories from User Service", synced_count);
    Ok(synced_count)
}

/// Get all categories from local mirror
pub async fn get_categories(pool: &PgPool) -> Result<Vec<StackCategory>, String> {
    let query_span = tracing::info_span!("get_categories");

    sqlx::query_as::<_, StackCategory>(
        r#"
        SELECT id, name, title, metadata
        FROM stack_category
        ORDER BY id
        "#
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch categories: {:?}", e);
        "Internal Server Error".to_string()
    })
}
