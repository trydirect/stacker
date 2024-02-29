use crate::models;
use sqlx::PgPool;
use tracing::Instrument;

pub async fn fetch(pool: &PgPool, id: i32) -> Result<Option<models::Server>, String> {
    tracing::info!("Fetch server {}", id);
    sqlx::query_as!(
        models::Server,
        r#"SELECT * FROM server WHERE id=$1 LIMIT 1 "#, id
    )
        .fetch_one(pool)
        .await
        .map(|server| Some(server))
        .or_else(|err| match err {
            sqlx::Error::RowNotFound => Ok(None),
            e => {
                tracing::error!("Failed to fetch server, error: {:?}", e);
                Err("Could not fetch data".to_string())
            }
        })
}

pub async fn fetch_by_user(pool: &PgPool, user_id: &str) -> Result<Vec<models::Server>, String> {
    let query_span = tracing::info_span!("Fetch servers by user id.");
    sqlx::query_as!(
        models::Server,
        r#"
        SELECT
            *
        FROM server
        WHERE user_id=$1
        "#,
        user_id
    )
        .fetch_all(pool)
        .instrument(query_span)
        .await
        .map_err(|err| {
            tracing::error!("Failed to fetch server, error: {:?}", err);
            "".to_string()
        })
}


pub async fn fetch_by_project(pool: &PgPool, project_id: i32) -> Result<Vec<models::Server>, String> {
    let query_span = tracing::info_span!("Fetch servers by project/project id.");
    sqlx::query_as!(
        models::Server,
        r#"
        SELECT
            *
        FROM server
        WHERE project_id=$1
        "#,
        project_id
    )
        .fetch_all(pool)
        .instrument(query_span)
        .await
        .map_err(|err| {
            tracing::error!("Failed to fetch servers, error: {:?}", err);
            "".to_string()
        })
}


pub async fn insert(pool: &PgPool, mut server: models::Server) -> Result<models::Server, String> {
    let query_span = tracing::info_span!("Saving user's server data into the database");
    sqlx::query!(
        r#"
        INSERT INTO server (
        user_id,
        cloud_id,
        project_id,
        region,
        zone,
        server,
        os,
        disk_type,
        created_at,
        updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING id;
        "#,
        server.user_id,
        server.cloud_id,
        server.project_id,
        server.region,
        server.zone,
        server.server,
        server.os,
        server.disk_type,
        server.created_at,
        server.updated_at,
    )
        .fetch_one(pool)
        .instrument(query_span)
        .await
        .map(move |result| {
            server.id = result.id;
            server
        })
        .map_err(|e| {
            tracing::error!("Failed to execute query: {:?}", e);
            "Failed to insert".to_string()
        })
}

pub async fn update(pool: &PgPool, mut server: models::Server) -> Result<models::Server, String> {
    let query_span = tracing::info_span!("Updating user server");
    sqlx::query_as!(
        models::Server,
        r#"
        UPDATE server
        SET
            user_id=$2,
            cloud_id=$3,
            project_id=$4,
            region=$5,
            zone=$6,
            server=$7,
            os=$8,
            disk_type=$9,
            created_at=$10,
            updated_at=NOW() at time zone 'utc'
        WHERE id = $1
        RETURNING *
        "#,
        server.id,
        server.user_id,
        server.cloud_id,
        server.project_id,
        server.region,
        server.zone,
        server.server,
        server.os,
        server.disk_type,
        server.created_at,
    )
        .fetch_one(pool)
        .instrument(query_span)
        .await
        .map(|result|{
            tracing::info!("Server info {} have been saved", server.id);
            server.updated_at = result.updated_at;
            server
        })
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            "".to_string()
        })
}
