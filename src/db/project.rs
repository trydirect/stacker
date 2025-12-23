use crate::models;
use sqlx::PgPool;
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

pub async fn fetch_one_by_name(pool: &PgPool, name: &str) -> Result<Option<models::Project>, String> {
    let query_span = tracing::info_span!("Fetch one project by name.");
    sqlx::query_as!(
        models::Project,
        r#"
        SELECT
            *
        FROM project
        WHERE name=$1
        LIMIT 1
        "#,
        name
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

pub async fn insert(pool: &PgPool, mut project: models::Project) -> Result<models::Project, String> {
    let query_span = tracing::info_span!("Saving new project into the database");
    sqlx::query!(
        r#"
        INSERT INTO project (stack_id, user_id, name, metadata, created_at, updated_at, request_json)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id;
        "#,
        project.stack_id,
        project.user_id,
        project.name,
        project.metadata,
        project.created_at,
        project.updated_at,
        project.request_json,
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(move |result| {
        project.id = result.id;
        project
    })
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        "Failed to insert".to_string()
    })
}

pub async fn update(pool: &PgPool, mut project: models::Project) -> Result<models::Project, String> {
    let query_span = tracing::info_span!("Updating project");
    sqlx::query_as!(
        models::Project,
        r#"
        UPDATE project
        SET 
            stack_id=$2,
            user_id=$3,
            name=$4,
            metadata=$5,
            request_json=$6,
            updated_at=NOW() at time zone 'utc'
        WHERE id = $1
        RETURNING *
        "#,
        project.id,
        project.stack_id,
        project.user_id,
        project.name,
        project.metadata,
        project.request_json
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(|result|{
        tracing::info!("Project {} has been saved to database", project.id);
        project.updated_at = result.updated_at;
        project
    })
    .map_err(|err| {
        tracing::error!("Failed to execute query: {:?}", err);
        "".to_string()
    })
}

#[tracing::instrument(name = "Delete user's project.")]
pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, String> {
    tracing::info!("Delete project {}", id);
    let mut tx = match pool.begin().await {
        Ok(result) => result,
        Err(err) => {
            tracing::error!("Failed to begin transaction: {:?}", err);
            return Err("".to_string());
        }
    };

    // Combine delete queries into a single query
    let delete_query = "
        --DELETE FROM deployment WHERE project_id = $1; // on delete cascade
        --DELETE FROM server WHERE project_id = $1; // on delete cascade
        DELETE FROM project WHERE id = $1;
    ";

    match sqlx::query(delete_query)
        .bind(id)
        .execute(&mut tx)
        .await
        .map_err(|err| {
            println!("{:?}", err)
        })
    {
        Ok(_) => {
            let _ = tx.commit().await.map_err(|err| {
                tracing::error!("Failed to commit transaction: {:?}", err);
                false
            });
            Ok(true)
        }
        Err(_err) => {
            let _ = tx.rollback().await.map_err(|err| println!("{:?}", err));
            Ok(false)
        }
        // todo, when empty commit()
    }
}

