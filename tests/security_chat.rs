mod common;

use common::{USER_A_ID, USER_A_TOKEN, USER_B_TOKEN};

/// Chat endpoints use (user_id, project_id) as the lookup key.
/// Isolation is enforced server-side: the handler always uses the authenticated
/// user's ID, so User B cannot see or mutate User A's chat history.

const TEST_PROJECT_ID: i32 = 9999;

async fn insert_chat(pool: &sqlx::PgPool, user_id: &str, project_id: i32) {
    sqlx::query(
        "INSERT INTO chat_conversations (id, user_id, project_id, messages) \
         VALUES (gen_random_uuid(), $1, $2, '[{\"role\":\"user\",\"content\":\"hello\"}]'::jsonb)",
    )
    .bind(user_id)
    .bind(project_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_list_chats_only_returns_own() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // User A saves a chat for a specific project
    insert_chat(&app.db_pool, USER_A_ID, TEST_PROJECT_ID).await;

    // User B queries the same project_id → should get 404 (no chat for B)
    let resp = client
        .get(format!(
            "{}/chat/history?project_id={}",
            &app.address, TEST_PROJECT_ID
        ))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status().as_u16(),
        404,
        "User B should not see User A's chat history"
    );
}

#[tokio::test]
async fn test_get_chat_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    insert_chat(&app.db_pool, USER_A_ID, TEST_PROJECT_ID).await;

    // User B GET on the same project_id → 404
    let resp = client
        .get(format!(
            "{}/chat/history?project_id={}",
            &app.address, TEST_PROJECT_ID
        ))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status().as_u16(),
        404,
        "User B GET on User A's chat should return 404"
    );
}

#[tokio::test]
async fn test_update_chat_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    insert_chat(&app.db_pool, USER_A_ID, TEST_PROJECT_ID).await;

    // User B upserts chat for the same project_id.
    // This should create a SEPARATE chat for User B, not overwrite User A's.
    let resp = client
        .put(format!("{}/chat/history", &app.address))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .header("Content-Type", "application/json")
        .body(
            serde_json::json!({
                "project_id": TEST_PROJECT_ID,
                "messages": [{"role": "user", "content": "attacker message"}]
            })
            .to_string(),
        )
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        resp.status().is_success(),
        "User B upsert should succeed (creates own chat)"
    );

    // Verify User A's chat is untouched
    let resp = client
        .get(format!(
            "{}/chat/history?project_id={}",
            &app.address, TEST_PROJECT_ID
        ))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let messages = body["item"]["messages"]
        .as_array()
        .expect("messages should be an array");
    assert_eq!(messages[0]["content"], "hello", "User A's chat must remain unchanged");
}

#[tokio::test]
async fn test_delete_chat_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    insert_chat(&app.db_pool, USER_A_ID, TEST_PROJECT_ID).await;

    // User B deletes → only deletes B's own (nonexistent) chat
    let resp = client
        .delete(format!(
            "{}/chat/history?project_id={}",
            &app.address, TEST_PROJECT_ID
        ))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    // Should succeed (no-op) but not affect A's data
    assert!(resp.status().is_success());

    // Verify User A's chat still exists
    let resp = client
        .get(format!(
            "{}/chat/history?project_id={}",
            &app.address, TEST_PROJECT_ID
        ))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        resp.status().is_success(),
        "User A's chat should survive User B's delete attempt"
    );
}

#[tokio::test]
async fn test_owner_can_access_own_chat() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // User A creates chat via API
    let resp = client
        .put(format!("{}/chat/history", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .header("Content-Type", "application/json")
        .body(
            serde_json::json!({
                "project_id": TEST_PROJECT_ID,
                "messages": [{"role": "user", "content": "my chat"}]
            })
            .to_string(),
        )
        .send()
        .await
        .expect("Failed to send request");
    assert!(
        resp.status().is_success(),
        "Owner should create chat, got {}",
        resp.status()
    );

    // User A can GET own chat
    let resp = client
        .get(format!(
            "{}/chat/history?project_id={}",
            &app.address, TEST_PROJECT_ID
        ))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");
    assert!(
        resp.status().is_success(),
        "Owner should read own chat, got {}",
        resp.status()
    );

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["item"]["user_id"], USER_A_ID);

    // User A can DELETE own chat
    let resp = client
        .delete(format!(
            "{}/chat/history?project_id={}",
            &app.address, TEST_PROJECT_ID
        ))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");
    assert!(
        resp.status().is_success(),
        "Owner should delete own chat, got {}",
        resp.status()
    );

    // Confirm it's gone
    let resp = client
        .get(format!(
            "{}/chat/history?project_id={}",
            &app.address, TEST_PROJECT_ID
        ))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");
    assert_eq!(resp.status().as_u16(), 404, "Deleted chat should be gone");
}
