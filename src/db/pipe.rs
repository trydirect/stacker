use crate::models::pipe::{PipeInstance, PipeTemplate};
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PipeTemplate queries
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Insert a new pipe template into the database
#[tracing::instrument(name = "Insert pipe template", skip(pool))]
pub async fn insert_template(
    pool: &PgPool,
    template: &PipeTemplate,
) -> Result<PipeTemplate, String> {
    let query_span = tracing::info_span!("Saving pipe template to database");
    sqlx::query_as::<_, PipeTemplate>(
        r#"
        INSERT INTO pipe_templates (
            id, name, description, source_app_type, source_endpoint,
            target_app_type, target_endpoint, target_external_url,
            field_mapping, config, is_public, created_by, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        RETURNING id, name, description, source_app_type, source_endpoint,
                  target_app_type, target_endpoint, target_external_url,
                  field_mapping, config, is_public, created_by, created_at, updated_at
        "#,
    )
    .bind(template.id)
    .bind(&template.name)
    .bind(&template.description)
    .bind(&template.source_app_type)
    .bind(&template.source_endpoint)
    .bind(&template.target_app_type)
    .bind(&template.target_endpoint)
    .bind(&template.target_external_url)
    .bind(&template.field_mapping)
    .bind(&template.config)
    .bind(template.is_public)
    .bind(&template.created_by)
    .bind(template.created_at)
    .bind(template.updated_at)
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to insert pipe template: {:?}", err);
        format!("Failed to insert pipe template: {}", err)
    })
}

/// Fetch a pipe template by ID
#[tracing::instrument(name = "Fetch pipe template by ID", skip(pool))]
pub async fn get_template(pool: &PgPool, id: &Uuid) -> Result<Option<PipeTemplate>, String> {
    let query_span = tracing::info_span!("Fetching pipe template by ID");
    sqlx::query_as::<_, PipeTemplate>(
        r#"
        SELECT id, name, description, source_app_type, source_endpoint,
               target_app_type, target_endpoint, target_external_url,
               field_mapping, config, is_public, created_by, created_at, updated_at
        FROM pipe_templates
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch pipe template: {:?}", err);
        format!("Failed to fetch pipe template: {}", err)
    })
}

/// Fetch a pipe template by name
#[tracing::instrument(name = "Fetch pipe template by name", skip(pool))]
pub async fn get_template_by_name(
    pool: &PgPool,
    name: &str,
) -> Result<Option<PipeTemplate>, String> {
    let query_span = tracing::info_span!("Fetching pipe template by name");
    sqlx::query_as::<_, PipeTemplate>(
        r#"
        SELECT id, name, description, source_app_type, source_endpoint,
               target_app_type, target_endpoint, target_external_url,
               field_mapping, config, is_public, created_by, created_at, updated_at
        FROM pipe_templates
        WHERE name = $1
        "#,
    )
    .bind(name)
    .fetch_optional(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch pipe template by name: {:?}", err);
        format!("Failed to fetch pipe template by name: {}", err)
    })
}

/// List pipe templates with optional filters
#[tracing::instrument(name = "List pipe templates", skip(pool))]
pub async fn list_templates(
    pool: &PgPool,
    source_app_type: Option<&str>,
    target_app_type: Option<&str>,
    public_only: bool,
) -> Result<Vec<PipeTemplate>, String> {
    let query_span = tracing::info_span!("Listing pipe templates");

    // Build dynamic query based on filters
    let mut sql = String::from(
        r#"
        SELECT id, name, description, source_app_type, source_endpoint,
               target_app_type, target_endpoint, target_external_url,
               field_mapping, config, is_public, created_by, created_at, updated_at
        FROM pipe_templates
        WHERE 1=1
        "#,
    );

    let mut param_idx = 1;
    if source_app_type.is_some() {
        sql.push_str(&format!(" AND source_app_type = ${}", param_idx));
        param_idx += 1;
    }
    if target_app_type.is_some() {
        sql.push_str(&format!(" AND target_app_type = ${}", param_idx));
        param_idx += 1;
    }
    if public_only {
        sql.push_str(&format!(" AND is_public = ${}", param_idx));
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut query = sqlx::query_as::<_, PipeTemplate>(&sql);

    if let Some(source) = source_app_type {
        query = query.bind(source.to_string());
    }
    if let Some(target) = target_app_type {
        query = query.bind(target.to_string());
    }
    if public_only {
        query = query.bind(true);
    }

    query
        .fetch_all(pool)
        .instrument(query_span)
        .await
        .map_err(|err| {
            tracing::error!("Failed to list pipe templates: {:?}", err);
            format!("Failed to list pipe templates: {}", err)
        })
}

/// Delete a pipe template by ID
#[tracing::instrument(name = "Delete pipe template", skip(pool))]
pub async fn delete_template(pool: &PgPool, id: &Uuid) -> Result<bool, String> {
    let query_span = tracing::info_span!("Deleting pipe template");
    let result = sqlx::query("DELETE FROM pipe_templates WHERE id = $1")
        .bind(id)
        .execute(pool)
        .instrument(query_span)
        .await
        .map_err(|err| {
            tracing::error!("Failed to delete pipe template: {:?}", err);
            format!("Failed to delete pipe template: {}", err)
        })?;

    Ok(result.rows_affected() > 0)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PipeInstance queries
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Insert a new pipe instance into the database
#[tracing::instrument(name = "Insert pipe instance", skip(pool))]
pub async fn insert_instance(
    pool: &PgPool,
    instance: &PipeInstance,
) -> Result<PipeInstance, String> {
    let query_span = tracing::info_span!("Saving pipe instance to database");
    sqlx::query_as::<_, PipeInstance>(
        r#"
        INSERT INTO pipe_instances (
            id, template_id, deployment_hash, source_container, target_container,
            target_url, field_mapping_override, config_override, status,
            last_triggered_at, trigger_count, error_count, created_by,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        RETURNING id, template_id, deployment_hash, source_container, target_container,
                  target_url, field_mapping_override, config_override, status,
                  last_triggered_at, trigger_count, error_count, created_by,
                  created_at, updated_at
        "#,
    )
    .bind(instance.id)
    .bind(instance.template_id)
    .bind(&instance.deployment_hash)
    .bind(&instance.source_container)
    .bind(&instance.target_container)
    .bind(&instance.target_url)
    .bind(&instance.field_mapping_override)
    .bind(&instance.config_override)
    .bind(&instance.status)
    .bind(instance.last_triggered_at)
    .bind(instance.trigger_count)
    .bind(instance.error_count)
    .bind(&instance.created_by)
    .bind(instance.created_at)
    .bind(instance.updated_at)
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to insert pipe instance: {:?}", err);
        format!("Failed to insert pipe instance: {}", err)
    })
}

/// Fetch a pipe instance by ID
#[tracing::instrument(name = "Fetch pipe instance by ID", skip(pool))]
pub async fn get_instance(pool: &PgPool, id: &Uuid) -> Result<Option<PipeInstance>, String> {
    let query_span = tracing::info_span!("Fetching pipe instance by ID");
    sqlx::query_as::<_, PipeInstance>(
        r#"
        SELECT id, template_id, deployment_hash, source_container, target_container,
               target_url, field_mapping_override, config_override, status,
               last_triggered_at, trigger_count, error_count, created_by,
               created_at, updated_at
        FROM pipe_instances
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch pipe instance: {:?}", err);
        format!("Failed to fetch pipe instance: {}", err)
    })
}

/// List pipe instances for a specific deployment
#[tracing::instrument(name = "List pipe instances for deployment", skip(pool))]
pub async fn list_instances(
    pool: &PgPool,
    deployment_hash: &str,
) -> Result<Vec<PipeInstance>, String> {
    let query_span = tracing::info_span!("Listing pipe instances for deployment");
    sqlx::query_as::<_, PipeInstance>(
        r#"
        SELECT id, template_id, deployment_hash, source_container, target_container,
               target_url, field_mapping_override, config_override, status,
               last_triggered_at, trigger_count, error_count, created_by,
               created_at, updated_at
        FROM pipe_instances
        WHERE deployment_hash = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(deployment_hash)
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to list pipe instances: {:?}", err);
        format!("Failed to list pipe instances: {}", err)
    })
}

/// Update the status of a pipe instance
#[tracing::instrument(name = "Update pipe instance status", skip(pool))]
pub async fn update_instance_status(
    pool: &PgPool,
    id: &Uuid,
    status: &str,
) -> Result<PipeInstance, String> {
    let query_span = tracing::info_span!("Updating pipe instance status");
    sqlx::query_as::<_, PipeInstance>(
        r#"
        UPDATE pipe_instances
        SET status = $2, updated_at = NOW()
        WHERE id = $1
        RETURNING id, template_id, deployment_hash, source_container, target_container,
                  target_url, field_mapping_override, config_override, status,
                  last_triggered_at, trigger_count, error_count, created_by,
                  created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(status)
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to update pipe instance status: {:?}", err);
        format!("Failed to update pipe instance status: {}", err)
    })
}

/// Delete a pipe instance by ID
#[tracing::instrument(name = "Delete pipe instance", skip(pool))]
pub async fn delete_instance(pool: &PgPool, id: &Uuid) -> Result<bool, String> {
    let query_span = tracing::info_span!("Deleting pipe instance");
    let result = sqlx::query("DELETE FROM pipe_instances WHERE id = $1")
        .bind(id)
        .execute(pool)
        .instrument(query_span)
        .await
        .map_err(|err| {
            tracing::error!("Failed to delete pipe instance: {:?}", err);
            format!("Failed to delete pipe instance: {}", err)
        })?;

    Ok(result.rows_affected() > 0)
}

/// Increment trigger count (and optionally error count) for a pipe instance
#[tracing::instrument(name = "Increment pipe trigger count", skip(pool))]
pub async fn increment_trigger_count(
    pool: &PgPool,
    id: &Uuid,
    success: bool,
) -> Result<(), String> {
    let query_span = tracing::info_span!("Incrementing pipe trigger count");

    let sql = if success {
        r#"
        UPDATE pipe_instances
        SET trigger_count = trigger_count + 1,
            last_triggered_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#
    } else {
        r#"
        UPDATE pipe_instances
        SET trigger_count = trigger_count + 1,
            error_count = error_count + 1,
            last_triggered_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#
    };

    sqlx::query(sql)
        .bind(id)
        .execute(pool)
        .instrument(query_span)
        .await
        .map_err(|err| {
            tracing::error!("Failed to increment pipe trigger count: {:?}", err);
            format!("Failed to increment pipe trigger count: {}", err)
        })
        .map(|_| ())
}
