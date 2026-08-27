mod common;

use common::{USER_A_ID, USER_A_TOKEN, USER_B_TOKEN};

/// Chat sessions are owned by the authenticated user. Every handler scopes its
/// query by the caller's user_id, so User B can never see, read, or delete
/// User A's sessions. Message content is encrypted at rest (AES-256-GCM, same
/// `Secret` helper used for cloud credentials), so the plaintext never lands in
/// the database.

/// Create a session directly for a user and return its id.
async fn insert_session(pool: &sqlx::PgPool, user_id: &str, plaintext_marker: &str) -> uuid::Uuid {
    // We insert through the API-equivalent path is not available here, so we
    // store a recognisable *plaintext* marker only for the "leak" assertion in
    // the DB. Real content is written encrypted by the handler; this row is a
    // control used purely for ownership checks, so an empty blob is fine.
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO chat_sessions (user_id, title, messages_encrypted) \
         VALUES ($1, $2, '') RETURNING id",
    )
    .bind(user_id)
    .bind(plaintext_marker)
    .fetch_one(pool)
    .await
    .unwrap();
    id
}

async fn create_session_via_api(
    address: &str,
    token: &str,
    content: &str,
) -> (reqwest::StatusCode, serde_json::Value) {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/chat/sessions", address))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(
            serde_json::json!({
                "title": "My Session",
                "messages": [{"role": "user", "content": content}]
            })
            .to_string(),
        )
        .send()
        .await
        .expect("Failed to send request");
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    (status, body)
}

#[tokio::test]
async fn test_owner_can_create_list_read_delete() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // Create
    let (status, body) = create_session_via_api(&app.address, USER_A_TOKEN, "hello secret").await;
    assert!(status.is_success(), "owner create failed: {}", status);
    let session_id = body["item"]["id"]
        .as_str()
        .expect("no session id")
        .to_string();

    // List returns the session but NOT message content
    let resp = client
        .get(format!("{}/chat/sessions", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let list_body = resp.text().await.unwrap();
    assert!(
        list_body.contains(&session_id),
        "owner should see own session in list"
    );
    assert!(
        !list_body.contains("hello secret"),
        "session list must not expose message content"
    );

    // Read messages
    let resp = client
        .get(format!(
            "{}/chat/sessions/{}/messages",
            &app.address, session_id
        ))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let msg_body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        msg_body["item"][0]["content"], "hello secret",
        "owner should read back decrypted messages"
    );

    // Delete
    let resp = client
        .delete(format!("{}/chat/sessions/{}", &app.address, session_id))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "owner delete failed");
}

#[tokio::test]
async fn test_messages_encrypted_at_rest() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let plaintext = "TOP-SECRET-PROMPT-9f3a";
    let (status, body) = create_session_via_api(&app.address, USER_A_TOKEN, plaintext).await;
    assert!(status.is_success());
    let session_id: uuid::Uuid = body["item"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .expect("session id should be a uuid");

    // The raw column must NOT contain the plaintext — it is AES-GCM ciphertext.
    let stored: String =
        sqlx::query_scalar("SELECT messages_encrypted FROM chat_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&app.db_pool)
            .await
            .unwrap();
    assert!(!stored.is_empty(), "encrypted blob should not be empty");
    assert!(
        !stored.contains(plaintext),
        "plaintext leaked into the database column: {}",
        stored
    );
}

#[tokio::test]
async fn test_list_only_returns_own_sessions() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let a_id = insert_session(&app.db_pool, USER_A_ID, "A session").await;

    // User B lists — must not see User A's session
    let resp = client
        .get(format!("{}/chat/sessions", &app.address))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body = resp.text().await.unwrap();
    assert!(
        !body.contains(&a_id.to_string()),
        "User B must not see User A's session in their list"
    );
}

#[tokio::test]
async fn test_read_other_users_session_is_404() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let a_id = insert_session(&app.db_pool, USER_A_ID, "A session").await;

    let resp = client
        .get(format!("{}/chat/sessions/{}/messages", &app.address, a_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B reading User A's session must 404 (no existence leak)"
    );
}

#[tokio::test]
async fn test_delete_other_users_session_is_404_and_no_op() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let a_id = insert_session(&app.db_pool, USER_A_ID, "A session").await;

    // User B tries to delete User A's session
    let resp = client
        .delete(format!("{}/chat/sessions/{}", &app.address, a_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B deleting User A's session must 404"
    );

    // Verify User A's session still exists
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_sessions WHERE id = $1")
        .bind(a_id)
        .fetch_one(&app.db_pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "User A's session must survive User B's delete attempt"
    );
}

#[tokio::test]
async fn test_append_persists_and_stays_encrypted() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (status, body) = create_session_via_api(&app.address, USER_A_TOKEN, "first message").await;
    assert!(status.is_success());
    let session_id = body["item"]["id"].as_str().unwrap().to_string();

    // Append a second message
    let secret_append = "APPENDED-SECRET-7c21";
    let resp = client
        .post(format!(
            "{}/chat/sessions/{}/messages",
            &app.address, session_id
        ))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .header("Content-Type", "application/json")
        .body(serde_json::json!({"role": "user", "content": secret_append}).to_string())
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "owner append failed: {}",
        resp.status()
    );
    let appended: serde_json::Value = resp.json().await.unwrap();
    let arr = appended["item"]
        .as_array()
        .expect("item should be the message array");
    assert_eq!(arr.len(), 2, "append should yield two messages");
    assert_eq!(arr[1]["content"], secret_append);

    // The appended content must be encrypted at rest, not plaintext
    let stored: String =
        sqlx::query_scalar("SELECT messages_encrypted FROM chat_sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&app.db_pool)
            .await
            .unwrap();
    assert!(
        !stored.contains(secret_append),
        "appended plaintext leaked into the database column"
    );
}

#[tokio::test]
async fn test_append_to_other_users_session_is_404() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let a_id = insert_session(&app.db_pool, USER_A_ID, "A session").await;

    let resp = client
        .post(format!("{}/chat/sessions/{}/messages", &app.address, a_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .header("Content-Type", "application/json")
        .body(serde_json::json!({"role": "user", "content": "intrusion"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B appending to User A's session must 404"
    );
}

#[tokio::test]
async fn test_rename_other_users_session_is_404() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let a_id = insert_session(&app.db_pool, USER_A_ID, "A session").await;

    let resp = client
        .patch(format!("{}/chat/sessions/{}", &app.address, a_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .header("Content-Type", "application/json")
        .body(serde_json::json!({"title": "hijacked"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B renaming User A's session must 404"
    );
}

#[tokio::test]
async fn test_archive_moves_session_between_lists() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (status, body) = create_session_via_api(&app.address, USER_A_TOKEN, "archive me").await;
    assert!(status.is_success());
    let session_id = body["item"]["id"].as_str().unwrap().to_string();

    // Archive it
    let resp = client
        .post(format!(
            "{}/chat/sessions/{}/archive",
            &app.address, session_id
        ))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "owner archive failed: {}",
        resp.status()
    );

    // Default (active) list must NOT contain it
    let active = client
        .get(format!("{}/chat/sessions", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !active.contains(&session_id),
        "archived session must not appear in the active list"
    );

    // Archived list MUST contain it
    let archived = client
        .get(format!("{}/chat/sessions?archived=true", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        archived.contains(&session_id),
        "archived session must appear in the archived list"
    );
}

#[tokio::test]
async fn test_archive_other_users_session_is_404() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let a_id = insert_session(&app.db_pool, USER_A_ID, "A session").await;

    let resp = client
        .post(format!("{}/chat/sessions/{}/archive", &app.address, a_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B archiving User A's session must 404"
    );
}
