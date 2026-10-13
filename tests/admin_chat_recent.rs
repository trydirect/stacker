//! Admin chat review must only expose chats of users who accepted the
//! `ai_chat_analysis` agreement, and must be closed to non-admins.
mod common;

use common::{USER_A_ID, USER_A_TOKEN, USER_B_ID};

async fn insert_conversation(pool: &sqlx::PgPool, user_id: &str, marker: &str) {
    sqlx::query(
        "INSERT INTO chat_conversations (user_id, messages) VALUES ($1, $2) \
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(serde_json::json!([{"role": "user", "content": marker}]))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn only_consenting_users_chats_are_returned() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let pool = &app.db_pool;
    insert_conversation(pool, USER_A_ID, "consented-marker").await;
    insert_conversation(pool, USER_B_ID, "private-marker").await;

    let agreement: i32 =
        sqlx::query_scalar("SELECT id FROM agreement WHERE name = 'ai_chat_analysis'")
            .fetch_one(pool)
            .await
            .expect("agreement seeded by migration");
    sqlx::query(
        "INSERT INTO user_agreement (agrt_id, user_id, created_at, updated_at) \
         VALUES ($1, $2, NOW(), NOW())",
    )
    .bind(agreement)
    .bind(USER_A_ID)
    .execute(pool)
    .await
    .unwrap();

    let chats = stacker::db::chat_admin::recent_consented(pool, 50)
        .await
        .unwrap();
    let dump = serde_json::to_string(&chats).unwrap();
    assert!(chats.iter().any(|c| c.user_id == USER_A_ID));
    assert!(chats.iter().all(|c| c.user_id != USER_B_ID), "{dump}");
}

#[tokio::test]
async fn non_admin_cannot_read_recent_chats() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let resp = reqwest::Client::new()
        .get(format!("{}/api/admin/chat/recent", app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403);
}

#[tokio::test]
async fn consent_agreement_and_casbin_rules_are_seeded() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let text: String =
        sqlx::query_scalar("SELECT text FROM agreement WHERE name = 'ai_chat_analysis'")
            .fetch_one(&app.db_pool)
            .await
            .expect("seeded agreement");
    assert!(text.contains("analyze my AI chat conversations"));

    for (role, path) in [
        ("group_admin", "/api/admin/chat/recent"),
        ("group_user", "/api/agreement/name/:name"),
        ("group_admin", "/api/agreement/name/:name"),
    ] {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM casbin_rule WHERE ptype='p' AND v0=$1 AND v1=$2 AND v2='GET'",
        )
        .bind(role)
        .bind(path)
        .fetch_one(&app.db_pool)
        .await
        .unwrap();
        assert_eq!(n, 1, "{role} {path}");
    }
}
