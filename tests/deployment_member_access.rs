//! The User Service asks Stacker which role a user holds on a deployment, so
//! a project member can operate the server (agent, apps, redeploy) through the
//! User Service installation endpoints, which only know the owner.
//!
//! The route is service-to-service: it answers only with STACKER_SERVICE_TOKEN
//! in `x-stacker-service-token`.

mod common;

use serde_json::Value;

const SERVICE_TOKEN: &str = "test-stacker-service-token";

async fn role_of(address: &str, hash: &str, user_id: &str, token: Option<&str>) -> (u16, Value) {
    let mut req = reqwest::Client::new().get(format!(
        "{address}/api/v1/deployments/{hash}/member-access?user_id={user_id}"
    ));
    if let Some(token) = token {
        req = req.header("x-stacker-service-token", token);
    }
    let resp = req.send().await.expect("request failed");
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

async fn add_member(pool: &sqlx::PgPool, project_id: i32, user_id: &str, role: &str) {
    sqlx::query(
        "INSERT INTO project_member (project_id, user_id, role, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(project_id)
    .bind(user_id)
    .bind(role)
    .bind(common::USER_A_ID)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_user_service_learns_each_users_role_on_a_deployment() {
    std::env::set_var("STACKER_SERVICE_TOKEN", SERVICE_TOKEN);
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let pool = &app.db_pool;
    let project = common::create_test_project(pool, common::USER_A_ID).await;
    let hash = format!("member-access-{}", uuid::Uuid::new_v4().simple());
    common::create_test_deployment(pool, common::USER_A_ID, project, &hash).await;
    add_member(pool, project, "member_user", "member").await;
    add_member(pool, project, "viewer_user", "viewer").await;

    for (user, expected) in [
        (common::USER_A_ID, Value::from("owner")),
        ("member_user", Value::from("member")),
        ("viewer_user", Value::from("viewer")),
        ("someone_else", Value::Null),
    ] {
        let (status, body) = role_of(&app.address, &hash, user, Some(SERVICE_TOKEN)).await;
        assert_eq!(status, 200, "{user}: {body}");
        assert_eq!(body["item"]["role"], expected, "{user}: {body}");
    }

    // An unknown deployment gives no role rather than an error.
    let (status, body) = role_of(
        &app.address,
        "no-such-hash",
        "member_user",
        Some(SERVICE_TOKEN),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["item"]["role"], Value::Null);
}

#[tokio::test]
async fn only_the_user_service_may_ask() {
    std::env::set_var("STACKER_SERVICE_TOKEN", SERVICE_TOKEN);
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let pool = &app.db_pool;
    let project = common::create_test_project(pool, common::USER_A_ID).await;
    let hash = format!("member-access-{}", uuid::Uuid::new_v4().simple());
    common::create_test_deployment(pool, common::USER_A_ID, project, &hash).await;

    let (status, _) = role_of(&app.address, &hash, common::USER_A_ID, None).await;
    assert_eq!(status, 403, "no service token");
    let (status, _) = role_of(&app.address, &hash, common::USER_A_ID, Some("wrong")).await;
    assert_eq!(status, 403, "wrong service token");

    // A signed-in user cannot ask about anyone either.
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/deployments/{hash}/member-access?user_id={}",
            app.address,
            common::USER_A_ID
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403, "user session");
}
