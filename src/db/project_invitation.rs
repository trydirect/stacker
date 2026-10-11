use crate::models;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// Advisory lock class for "seats of one project". Taken for the length of a
/// transaction so two invitations cannot both take the last free seat.
const SEAT_LOCK_CLASS: i32 = 0x5345_4154; // "SEAT"

fn db_error(context: &str) -> impl Fn(sqlx::Error) -> String + '_ {
    move |err| {
        tracing::error!("{}: {:?}", context, err);
        context.to_string()
    }
}

pub async fn lock_project_seats<'e, E: PgExecutor<'e>>(
    executor: E,
    project_id: i32,
) -> Result<(), String> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock($1, $2)",
        SEAT_LOCK_CLASS,
        project_id
    )
    .fetch_one(executor)
    .await
    .map(|_| ())
    .map_err(db_error("Failed to lock project seats"))
}

/// Mark pending invitations past their expiry as expired, so they stop
/// holding a seat and the address can be invited again.
pub async fn expire_stale<'e, E: PgExecutor<'e>>(
    executor: E,
    project_id: i32,
) -> Result<(), String> {
    sqlx::query!(
        r#"
        UPDATE project_invitation SET status = 'expired'
        WHERE project_id = $1 AND status = 'pending' AND expires_at <= NOW()
        "#,
        project_id
    )
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(db_error("Failed to expire stale invitations"))
}

/// Seats in use: the owner, every member, and every live pending invitation.
pub async fn seats_taken<'e, E: PgExecutor<'e>>(
    executor: E,
    project_id: i32,
) -> Result<i64, String> {
    sqlx::query_scalar!(
        r#"
        SELECT (
            1
            + (SELECT COUNT(*) FROM project_member WHERE project_id = $1)
            + (SELECT COUNT(*) FROM project_invitation
               WHERE project_id = $1 AND status = 'pending' AND expires_at > NOW())
        )::BIGINT AS "taken!"
        "#,
        project_id
    )
    .fetch_one(executor)
    .await
    .map_err(db_error("Failed to count project seats"))
}

pub async fn live_pending_exists<'e, E: PgExecutor<'e>>(
    executor: E,
    project_id: i32,
    email: &str,
) -> Result<bool, String> {
    sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM project_invitation
            WHERE project_id = $1 AND email = $2
              AND status = 'pending' AND expires_at > NOW()
        ) AS "exists!"
        "#,
        project_id,
        email
    )
    .fetch_one(executor)
    .await
    .map_err(db_error("Failed to check pending invitations"))
}

pub struct NewInvitation<'a> {
    pub project_id: i32,
    pub email: &'a str,
    pub role: &'a str,
    pub token_hash: &'a str,
    pub invited_by: &'a str,
    pub invited_by_email: &'a str,
    pub valid_for_days: i32,
}

pub async fn insert<'e, E: PgExecutor<'e>>(
    executor: E,
    new: NewInvitation<'_>,
) -> Result<models::ProjectInvitation, String> {
    sqlx::query_as!(
        models::ProjectInvitation,
        r#"
        INSERT INTO project_invitation
            (id, project_id, email, role, token_hash, invited_by, invited_by_email,
             status, expires_at, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending',
                NOW() + make_interval(days => $8), NOW())
        RETURNING id, project_id, email, role, token_hash, invited_by, invited_by_email,
                  status, expires_at, accepted_by, accepted_at, created_at
        "#,
        Uuid::new_v4(),
        new.project_id,
        new.email,
        new.role,
        new.token_hash,
        new.invited_by,
        new.invited_by_email,
        new.valid_for_days
    )
    .fetch_one(executor)
    .await
    .map_err(db_error("Failed to save invitation"))
}

pub async fn list_live_pending(
    pool: &PgPool,
    project_id: i32,
) -> Result<Vec<models::ProjectInvitation>, String> {
    sqlx::query_as!(
        models::ProjectInvitation,
        r#"
        SELECT id, project_id, email, role, token_hash, invited_by, invited_by_email,
               status, expires_at, accepted_by, accepted_at, created_at
        FROM project_invitation
        WHERE project_id = $1 AND status = 'pending' AND expires_at > NOW()
        ORDER BY created_at ASC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .map_err(db_error("Failed to list invitations"))
}

pub async fn fetch_by_token_hash(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<models::ProjectInvitation>, String> {
    sqlx::query_as!(
        models::ProjectInvitation,
        r#"
        SELECT id, project_id, email, role, token_hash, invited_by, invited_by_email,
               status, expires_at, accepted_by, accepted_at, created_at
        FROM project_invitation
        WHERE token_hash = $1
        "#,
        token_hash
    )
    .fetch_optional(pool)
    .await
    .map_err(db_error("Failed to fetch invitation"))
}

/// Fetch and lock one invitation for the rest of the transaction.
pub async fn lock_by_id<'e, E: PgExecutor<'e>>(
    executor: E,
    id: Uuid,
) -> Result<Option<models::ProjectInvitation>, String> {
    sqlx::query_as!(
        models::ProjectInvitation,
        r#"
        SELECT id, project_id, email, role, token_hash, invited_by, invited_by_email,
               status, expires_at, accepted_by, accepted_at, created_at
        FROM project_invitation
        WHERE id = $1
        FOR UPDATE
        "#,
        id
    )
    .fetch_optional(executor)
    .await
    .map_err(db_error("Failed to fetch invitation"))
}

/// Revoke a pending invitation of this project. False when there is none.
pub async fn revoke(pool: &PgPool, project_id: i32, id: Uuid) -> Result<bool, String> {
    sqlx::query!(
        r#"
        UPDATE project_invitation SET status = 'revoked'
        WHERE id = $1 AND project_id = $2 AND status = 'pending'
        "#,
        id,
        project_id
    )
    .execute(pool)
    .await
    .map(|result| result.rows_affected() > 0)
    .map_err(db_error("Failed to revoke invitation"))
}

pub async fn set_status<'e, E: PgExecutor<'e>>(
    executor: E,
    id: Uuid,
    status: &str,
    accepted_by: Option<&str>,
) -> Result<(), String> {
    sqlx::query!(
        r#"
        UPDATE project_invitation
        SET status = $2::VARCHAR,
            accepted_by = $3,
            accepted_at = CASE WHEN $2::VARCHAR = 'accepted' THEN NOW() ELSE accepted_at END
        WHERE id = $1
        "#,
        id,
        status,
        accepted_by
    )
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(db_error("Failed to update invitation"))
}

pub async fn pending_for_email(
    pool: &PgPool,
    email: &str,
) -> Result<Vec<models::PendingProjectInvitation>, String> {
    sqlx::query_as!(
        models::PendingProjectInvitation,
        r#"
        SELECT i.id, i.project_id, p.name AS project_name, i.role,
               i.invited_by_email AS invited_by, i.expires_at
        FROM project_invitation i
        JOIN project p ON p.id = i.project_id
        WHERE i.email = $1 AND i.status = 'pending' AND i.expires_at > NOW()
        ORDER BY i.created_at DESC
        "#,
        email
    )
    .fetch_all(pool)
    .await
    .map_err(db_error("Failed to list pending invitations"))
}

/// Add a member unless they already are one. True when a row was added.
pub async fn insert_member<'e, E: PgExecutor<'e>>(
    executor: E,
    project_id: i32,
    user_id: &str,
    role: &str,
    created_by: &str,
) -> Result<bool, String> {
    sqlx::query!(
        r#"
        INSERT INTO project_member (project_id, user_id, role, created_by, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW() at time zone 'utc', NOW() at time zone 'utc')
        ON CONFLICT (project_id, user_id) DO NOTHING
        "#,
        project_id,
        user_id,
        role,
        created_by
    )
    .execute(executor)
    .await
    .map(|result| result.rows_affected() > 0)
    .map_err(db_error("Failed to add project member"))
}
