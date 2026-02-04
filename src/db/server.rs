use crate::models;
use sqlx::PgPool;
use tracing::Instrument;

pub async fn fetch(pool: &PgPool, id: i32) -> Result<Option<models::Server>, String> {
    tracing::info!("Fetch server {}", id);
    sqlx::query_as!(
        models::Server,
        r#"SELECT * FROM server WHERE id=$1 LIMIT 1 "#,
        id
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

pub async fn fetch_by_project(
    pool: &PgPool,
    project_id: i32,
) -> Result<Vec<models::Server>, String> {
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
        project_id,
        region,
        zone,
        server,
        os,
        disk_type,
        created_at,
        updated_at,
        srv_ip,
        ssh_user,
        ssh_port,
        vault_key_path,
        connection_mode,
        key_status,
        name
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW() at time zone 'utc',NOW() at time zone 'utc', $8, $9, $10, $11, $12, $13, $14)
        RETURNING id;
        "#,
        server.user_id,
        server.project_id,
        server.region,
        server.zone,
        server.server,
        server.os,
        server.disk_type,
        server.srv_ip,
        server.ssh_user,
        server.ssh_port,
        server.vault_key_path,
        server.connection_mode,
        server.key_status,
        server.name
    )
        .fetch_one(pool)
        .instrument(query_span)
        .await
        .map(move |result| {
            server.id = result.id;
            server
        })
        .map_err(|e| {

            // match err {
            // sqlx::error::ErrorKind::ForeignKeyViolation => {
            //     return JsonResponse::<models::Server>::build().bad_request("");
            // }
            //     _ => {
            //         return JsonResponse::<models::Server>::build().internal_server_error("Failed to insert");
            //     }
            // })
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
            project_id=$3,
            region=$4,
            zone=$5,
            server=$6,
            os=$7,
            disk_type=$8,
            updated_at=NOW() at time zone 'utc',
            srv_ip=$9,
            ssh_user=$10,
            ssh_port=$11,
            vault_key_path=$12,
            connection_mode=$13,
            key_status=$14,
            name=$15
        WHERE id = $1
        RETURNING *
        "#,
        server.id,
        server.user_id,
        server.project_id,
        server.region,
        server.zone,
        server.server,
        server.os,
        server.disk_type,
        server.srv_ip,
        server.ssh_user,
        server.ssh_port,
        server.vault_key_path,
        server.connection_mode,
        server.key_status,
        server.name
    )
    .fetch_one(pool)
    .instrument(query_span)
    .await
    .map(|result| {
        tracing::info!("Server info {} have been saved", server.id);
        server.updated_at = result.updated_at;
        server
    })
    .map_err(|err| {
        tracing::error!("Failed to execute query: {:?}", err);
        "".to_string()
    })
}

/// Update SSH key status and vault path for a server
#[tracing::instrument(name = "Update server SSH key status.")]
pub async fn update_ssh_key_status(
    pool: &PgPool,
    server_id: i32,
    vault_key_path: Option<String>,
    key_status: &str,
) -> Result<models::Server, String> {
    sqlx::query_as!(
        models::Server,
        r#"
        UPDATE server
        SET
            vault_key_path = $2,
            key_status = $3,
            updated_at = NOW() at time zone 'utc'
        WHERE id = $1
        RETURNING *
        "#,
        server_id,
        vault_key_path,
        key_status
    )
    .fetch_one(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to update SSH key status: {:?}", err);
        "Failed to update SSH key status".to_string()
    })
}

/// Update connection mode for a server
#[tracing::instrument(name = "Update server connection mode.")]
pub async fn update_connection_mode(
    pool: &PgPool,
    server_id: i32,
    connection_mode: &str,
) -> Result<models::Server, String> {
    sqlx::query_as!(
        models::Server,
        r#"
        UPDATE server
        SET
            connection_mode = $2,
            updated_at = NOW() at time zone 'utc'
        WHERE id = $1
        RETURNING *
        "#,
        server_id,
        connection_mode
    )
    .fetch_one(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to update connection mode: {:?}", err);
        "Failed to update connection mode".to_string()
    })
}

#[tracing::instrument(name = "Delete user's server.")]
pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, String> {
    tracing::info!("Delete server {}", id);
    sqlx::query::<sqlx::Postgres>("DELETE FROM server WHERE id = $1;")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| true)
        .map_err(|err| {
            tracing::error!("Failed to delete server: {:?}", err);
            "Failed to delete server".to_string()
        })
}
