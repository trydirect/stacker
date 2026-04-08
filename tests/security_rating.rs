mod common;

use common::{USER_A_ID, USER_A_TOKEN, USER_B_TOKEN};
use sqlx::Row;

/// Rating edit/delete endpoints check `rating.user_id == user.id`.
/// Non-owner attempts return 404 (the handler treats missing-or-not-owned as "not found").

async fn insert_rating(pool: &sqlx::PgPool, user_id: &str) -> i32 {
    let rec = sqlx::query(
        "INSERT INTO rating (user_id, obj_id, rating, comment, category) \
         VALUES ($1, 1, 5, 'great', 'Application') RETURNING id",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    rec.get("id")
}

#[tokio::test]
async fn test_edit_rating_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let rating_id = insert_rating(&app.db_pool, USER_A_ID).await;

    let resp = client
        .put(format!("{}/rating/{}", &app.address, rating_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .header("Content-Type", "application/json")
        .body(serde_json::json!({"comment": "hacked", "rate": 1}).to_string())
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status().as_u16(),
        404,
        "User B editing User A's rating should return 404"
    );
}

#[tokio::test]
async fn test_delete_rating_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let rating_id = insert_rating(&app.db_pool, USER_A_ID).await;

    let resp = client
        .delete(format!("{}/rating/{}", &app.address, rating_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status().as_u16(),
        404,
        "User B deleting User A's rating should return 404"
    );

    // Verify the rating is still intact
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM rating WHERE id = $1 AND hidden = false")
            .bind(rating_id)
            .fetch_one(&app.db_pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "Rating should not be deleted by non-owner");
}

#[tokio::test]
async fn test_owner_can_edit_own_rating() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let rating_id = insert_rating(&app.db_pool, USER_A_ID).await;

    // Owner edits the rating
    let resp = client
        .put(format!("{}/rating/{}", &app.address, rating_id))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .header("Content-Type", "application/json")
        .body(serde_json::json!({"comment": "updated comment", "rate": 8}).to_string())
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        resp.status().is_success(),
        "Owner should edit own rating, got {}",
        resp.status()
    );

    // Owner deletes (soft-delete) the rating
    let resp = client
        .delete(format!("{}/rating/{}", &app.address, rating_id))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        resp.status().is_success(),
        "Owner should delete own rating, got {}",
        resp.status()
    );
}
