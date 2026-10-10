use crate::models;
use sqlx::PgPool;
use tracing::Instrument;

pub async fn fetch(pool: &PgPool, id: i32) -> Result<Option<models::Server>, String> {
    tracing::info!("Fetch server {}", id);
    sqlx::query_as!(
        models::Server,
        r#"
        SELECT
            id,
            user_id,
            project_id,
            cloud_id,
            region,
            zone,
            server,
            os,
            disk_type,
            created_at,
            updated_at,
            srv_ip,
            ssh_port,
            ssh_user,
            vault_key_path,
            connection_mode,
            key_status,
            name,
            host_key_fingerprint
        FROM server
        WHERE id=$1
        LIMIT 1
        "#,
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
            id,
            user_id,
            project_id,
            cloud_id,
            region,
            zone,
            server,
            os,
            disk_type,
            created_at,
            updated_at,
            srv_ip,
            ssh_port,
            ssh_user,
            vault_key_path,
            connection_mode,
            key_status,
            name,
            host_key_fingerprint
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

/// Fetch servers by user ID with cloud provider information
pub async fn fetch_by_user_with_provider(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<models::ServerWithProvider>, String> {
    let query_span = tracing::info_span!("Fetch servers by user id with provider info.");
    sqlx::query_as!(
        models::ServerWithProvider,
        r#"
        SELECT
            s.id,
            s.user_id,
            s.project_id,
            s.cloud_id,
            c.provider as "cloud?: String",
            s.region,
            s.zone,
            s.server,
            s.os,
            s.disk_type,
            s.created_at,
            s.updated_at,
            s.srv_ip,
            s.ssh_port,
            s.ssh_user,
            s.vault_key_path,
            s.connection_mode,
            s.key_status,
            s.name
        FROM server s
        LEFT JOIN cloud c ON s.cloud_id = c.id
        WHERE s.user_id=$1
        ORDER BY s.created_at DESC
        "#,
        user_id
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch servers with provider, error: {:?}", err);
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
            id,
            user_id,
            project_id,
            cloud_id,
            region,
            zone,
            server,
            os,
            disk_type,
            created_at,
            updated_at,
            srv_ip,
            ssh_port,
            ssh_user,
            vault_key_path,
            connection_mode,
            key_status,
            name,
            host_key_fingerprint
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
        cloud_id,
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
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW() at time zone 'utc',NOW() at time zone 'utc', $9, $10, $11, $12, $13, $14, $15)
        RETURNING id;
        "#,
        server.user_id,
        server.project_id,
        server.cloud_id,
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
            cloud_id=$4,
            region=$5,
            zone=$6,
            server=$7,
            os=$8,
            disk_type=$9,
            updated_at=NOW() at time zone 'utc',
            srv_ip=$10,
            ssh_user=$11,
            ssh_port=$12,
            vault_key_path=$13,
            connection_mode=$14,
            key_status=$15,
            name=$16
        WHERE id = $1
        RETURNING
            id,
            user_id,
            project_id,
            cloud_id,
            region,
            zone,
            server,
            os,
            disk_type,
            created_at,
            updated_at,
            srv_ip,
            ssh_port,
            ssh_user,
            vault_key_path,
            connection_mode,
            key_status,
            name,
            host_key_fingerprint
        "#,
        server.id,
        server.user_id,
        server.project_id,
        server.cloud_id,
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
        RETURNING
            id,
            user_id,
            project_id,
            cloud_id,
            region,
            zone,
            server,
            os,
            disk_type,
            created_at,
            updated_at,
            srv_ip,
            ssh_port,
            ssh_user,
            vault_key_path,
            connection_mode,
            key_status,
            name,
            host_key_fingerprint
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
#[allow(dead_code)]
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
        RETURNING
            id,
            user_id,
            project_id,
            cloud_id,
            region,
            zone,
            server,
            os,
            disk_type,
            created_at,
            updated_at,
            srv_ip,
            ssh_port,
            ssh_user,
            vault_key_path,
            connection_mode,
            key_status,
            name,
            host_key_fingerprint
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

/// Update server IP and SSH port after a successful cloud deployment.
/// Called by the MQ listener when a "completed" status message includes srv_ip.
#[tracing::instrument(name = "Update server IP from deployment completion.")]
pub async fn update_srv_ip(
    pool: &PgPool,
    project_id: i32,
    srv_ip: &str,
    ssh_port: Option<i32>,
) -> Result<models::Server, String> {
    sqlx::query_as!(
        models::Server,
        r#"
        UPDATE server
        SET
            srv_ip = $2,
            ssh_port = COALESCE($3, ssh_port),
            -- A new address means a new machine, so a pin for the old one must
            -- not carry over; a re-provisioned server legitimately presents a
            -- new host key. $2 is cast because it is also compared below.
            host_key_fingerprint = CASE
                WHEN server.srv_ip IS DISTINCT FROM $2::varchar THEN NULL
                ELSE server.host_key_fingerprint
            END,
            updated_at = NOW() at time zone 'utc'
        WHERE project_id = $1
        RETURNING
            id,
            user_id,
            project_id,
            cloud_id,
            region,
            zone,
            server,
            os,
            disk_type,
            created_at,
            updated_at,
            srv_ip,
            ssh_port,
            ssh_user,
            vault_key_path,
            connection_mode,
            key_status,
            name,
            host_key_fingerprint
        "#,
        project_id,
        Some(srv_ip.to_string()),
        ssh_port,
    )
    .fetch_one(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to update server IP: {:?}", err);
        "Failed to update server IP".to_string()
    })
}

/// Pin the SSH host key fingerprint observed for a server (trust on first use).
///
/// Only ever called when the stored pin was empty and a connection succeeded.
/// An existing pin is never overwritten here: a mismatch is refused at connect
/// time instead, so a server that starts presenting a different key needs a
/// deliberate re-pin rather than silently adopting the new one.
#[tracing::instrument(name = "Pin server SSH host key fingerprint.")]
pub async fn update_host_key_fingerprint(
    pool: &PgPool,
    server_id: i32,
    host_key_fingerprint: &str,
) -> Result<models::Server, String> {
    sqlx::query_as!(
        models::Server,
        r#"
        UPDATE server
        SET
            host_key_fingerprint = $2,
            updated_at = NOW() at time zone 'utc'
        WHERE id = $1
          AND host_key_fingerprint IS NULL
        RETURNING
            id,
            user_id,
            project_id,
            cloud_id,
            region,
            zone,
            server,
            os,
            disk_type,
            created_at,
            updated_at,
            srv_ip,
            ssh_port,
            ssh_user,
            vault_key_path,
            connection_mode,
            key_status,
            name,
            host_key_fingerprint
        "#,
        server_id,
        host_key_fingerprint,
    )
    .fetch_one(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to pin server host key fingerprint: {:?}", err);
        "Failed to pin server host key fingerprint".to_string()
    })
}

#[tracing::instrument(name = "Delete user's server.")]
pub async fn delete(pool: &PgPool, id: i32, user_id: &str) -> Result<bool, String> {
    tracing::info!("Delete server {}", id);
    sqlx::query::<sqlx::Postgres>("DELETE FROM server WHERE id = $1 AND user_id = $2;")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|err| {
            tracing::error!("Failed to delete server: {:?}", err);
            "Failed to delete server".to_string()
        })
}
