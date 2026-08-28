use crate::models;
use sqlx::PgPool;
use sqlx::Row;
use tracing::Instrument;

pub async fn fetch(pool: &PgPool, id: i32) -> Result<Option<models::Project>, String> {
    tracing::info!("Fetch project {}", id);
    sqlx::query_as!(
        models::Project,
        r#"
        SELECT
            *
        FROM project
        WHERE id=$1
        LIMIT 1
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .map(|project| Some(project))
    .or_else(|err| match err {
        sqlx::Error::RowNotFound => Ok(None),
        e => {
            tracing::error!("Failed to fetch project, error: {:?}", e);
            Err("Could not fetch data".to_string())
        }
    })
}

pub async fn fetch_by_user(pool: &PgPool, user_id: &str) -> Result<Vec<models::Project>, String> {
    let query_span = tracing::info_span!("Fetch projects by user id.");
    sqlx::query_as!(
        models::Project,
        r#"
        SELECT
            *
        FROM project
        WHERE user_id=$1
        "#,
        user_id
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch project, error: {:?}", err);
        "".to_string()
    })
}

pub async fn fetch_shared_by_user(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<models::SharedProjectSummary>, String> {
    let query_span = tracing::info_span!("Fetch shared projects by user id.");
    sqlx::query_as::<_, models::SharedProjectSummary>(
        r#"
        SELECT
            p.id,
            p.name,
            pm.role,
            pm.created_at AS shared_at
        FROM project_member pm
        JOIN project p ON p.id = pm.project_id
        WHERE pm.user_id = $1
        ORDER BY pm.created_at DESC, p.id DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch shared projects, error: {:?}", err);
        "".to_string()
    })
}

pub async fn fetch_one_by_name(
    pool: &PgPool,
    name: &str,
    user_id: &str,
) -> Result<Option<models::Project>, String> {
    let query_span = tracing::info_span!("Fetch one project by name.");
    sqlx::query_as!(
        models::Project,
        r#"
        SELECT
            *
        FROM project
        WHERE name=$1 AND user_id=$2
        LIMIT 1
        "#,
        name,
        user_id
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(|project| Some(project))
    .or_else(|err| match err {
        sqlx::Error::RowNotFound => Ok(None),
        err => {
            tracing::error!("Failed to fetch one project by name, error: {:?}", err);
            Err("".to_string())
        }
    })
}

pub async fn insert(
    pool: &PgPool,
    mut project: models::Project,
) -> Result<models::Project, String> {
    let query_span = tracing::info_span!("Saving new project into the database");
    sqlx::query(
        r#"
        INSERT INTO project (
            stack_id,
            user_id,
            name,
            metadata,
            created_at,
            updated_at,
            request_json,
            source_template_id,
            template_version,
            is_protected,
            deletion_scheduled_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id;
        "#,
    )
    .bind(project.stack_id)
    .bind(project.user_id.clone())
    .bind(project.name.clone())
    .bind(project.metadata.clone())
    .bind(project.created_at)
    .bind(project.updated_at)
    .bind(project.request_json.clone())
    .bind(project.source_template_id)
    .bind(project.template_version.clone())
    .bind(project.is_protected)
    .bind(project.deletion_scheduled_at)
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(move |result| {
        project.id = result.get("id");
        project
    })
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        "Failed to insert".to_string()
    })
}

pub async fn update(
    pool: &PgPool,
    mut project: models::Project,
) -> Result<models::Project, String> {
    let query_span = tracing::info_span!("Updating project");
    sqlx::query(
        r#"
        UPDATE project
        SET 
            stack_id=$2,
            user_id=$3,
            name=$4,
            metadata=$5,
            request_json=$6,
            source_template_id=$7,
            template_version=$8,
            is_protected=$9,
            updated_at=NOW() at time zone 'utc',
            deletion_scheduled_at=NULL
        WHERE id = $1
        "#,
    )
    .bind(project.id)
    .bind(project.stack_id)
    .bind(project.user_id.clone())
    .bind(project.name.clone())
    .bind(project.metadata.clone())
    .bind(project.request_json.clone())
    .bind(project.source_template_id)
    .bind(project.template_version.clone())
    .bind(project.is_protected)
    .execute(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to execute query: {:?}", err);
        "".to_string()
    })?;

    fetch(pool, project.id)
        .await
        .and_then(|result| result.ok_or_else(|| "Project not found after update".to_string()))
        .map(|saved| {
            tracing::info!("Project {} has been saved to database", project.id);
            project.updated_at = saved.updated_at;
            project
        })
}

#[tracing::instrument(name = "Delete user's project.")]
pub async fn delete(pool: &PgPool, id: i32, user_id: &str) -> Result<bool, String> {
    tracing::info!("Delete project {}", id);
    sqlx::query::<sqlx::Postgres>("DELETE FROM project WHERE id = $1 AND user_id = $2;")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|err| {
            tracing::error!("Failed to delete project: {:?}", err);
            "Failed to delete project".to_string()
        })
}

/// Set or unset the `is_protected` flag on a project.
/// Returns `true` if the row was updated, `false` if no matching project was found.
pub async fn set_protected(
    pool: &PgPool,
    id: i32,
    user_id: &str,
    protected: bool,
) -> Result<bool, String> {
    tracing::info!("Set project {} is_protected={}", id, protected);
    sqlx::query(
        "UPDATE project SET is_protected = $3, updated_at = NOW() at time zone 'utc' WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .bind(protected)
    .execute(pool)
    .await
    .map(|r| r.rows_affected() > 0)
    .map_err(|err| {
        tracing::error!("Failed to set project protection: {:?}", err);
        "Failed to update project protection".to_string()
    })
}

/// Summary of resources that block project deletion.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeletionBlockers {
    pub is_protected: bool,
    pub has_marketplace_template: bool,
    pub active_deployments: i64,
    pub active_servers: i64,
}

/// Check what resources would block deletion of a project.
pub async fn check_deletion_blockers(
    pool: &PgPool,
    project_id: i32,
) -> Result<DeletionBlockers, String> {
    let row = sqlx::query(
        r#"
        SELECT
            p.is_protected,
            p.source_template_id IS NOT NULL AS has_marketplace_template,
            COALESCE(d.cnt, 0) AS active_deployments,
            COALESCE(s.cnt, 0) AS active_servers
        FROM project p
        LEFT JOIN (
            SELECT project_id, COUNT(*) AS cnt
            FROM deployment
            WHERE deleted = false OR deleted IS NULL
            GROUP BY project_id
        ) d ON d.project_id = p.id
        LEFT JOIN (
            SELECT project_id, COUNT(*) AS cnt
            FROM server
            GROUP BY project_id
        ) s ON s.project_id = p.id
        WHERE p.id = $1
        "#,
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to check deletion blockers: {:?}", err);
        "Failed to check deletion blockers".to_string()
    })?;

    Ok(DeletionBlockers {
        is_protected: row.get("is_protected"),
        has_marketplace_template: row.get("has_marketplace_template"),
        active_deployments: row.get::<i64, _>("active_deployments"),
        active_servers: row.get::<i64, _>("active_servers"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deletion_blockers_serialization() {
        let blockers = DeletionBlockers {
            is_protected: true,
            has_marketplace_template: true,
            active_deployments: 3,
            active_servers: 2,
        };
        let json = serde_json::to_value(&blockers).unwrap();
        assert_eq!(json["is_protected"], true);
        assert_eq!(json["has_marketplace_template"], true);
        assert_eq!(json["active_deployments"], 3);
        assert_eq!(json["active_servers"], 2);
    }

    #[test]
    fn test_deletion_blockers_zero_counts() {
        let blockers = DeletionBlockers {
            is_protected: false,
            has_marketplace_template: false,
            active_deployments: 0,
            active_servers: 0,
        };
        let json = serde_json::to_value(&blockers).unwrap();
        assert_eq!(json["is_protected"], false);
        assert_eq!(json["active_deployments"], 0);
        assert_eq!(json["active_servers"], 0);
    }
}
