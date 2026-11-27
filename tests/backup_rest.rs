//! Integration tests for the backup REST API (WS-1 1.5): destinations, jobs,
//! runs — plus the IDOR guard (another user's job answers 404 like a missing).

mod common;

use serde_json::{json, Value};

fn auth(user_token: &str) -> String {
    format!("Bearer {user_token}")
}

async fn create_destination(
    address: &str,
    token: &str,
    name: &str,
    endpoint: &str,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{address}/api/v1/backups/destinations"))
        .header("Authorization", auth(token))
        .json(&json!({
            "name": name,
            "endpoint": endpoint,
            "vaultPath": format!("backup/{name}"),
        }))
        .send()
        .await
        .expect("create destination")
}

async fn create_job(
    address: &str,
    token: &str,
    project_id: i32,
    destination_id: i32,
    name: &str,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{address}/api/v1/backups/jobs"))
        .header("Authorization", auth(token))
        .json(&json!({
            "projectId": project_id,
            "destinationId": destination_id,
            "name": name,
            "source": "postgres",
            "target": "shop-postgres-1",
            "database": "shop",
            "schedule": "0 3 * * *",
        }))
        .send()
        .await
        .expect("create job")
}

async fn body_of(response: reqwest::Response) -> Value {
    response.json().await.expect("json body")
}

#[tokio::test]
async fn backup_destination_job_and_run_rest_roundtrip() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let address = &app.address;
    let token = common::USER_A_TOKEN;
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;

    // Destination: created, masked endpoint (host only), no vault_path leaked.
    let resp = create_destination(address, token, "primary", "https://api.backup.example/v1").await;
    assert_eq!(resp.status().as_u16(), 201);
    let dest = body_of(resp).await;
    assert_eq!(dest["item"]["name"], "primary");
    assert_eq!(dest["item"]["endpoint"], "api.backup.example");
    let serialized = dest.to_string();
    assert!(
        !serialized.contains("vaultPath"),
        "vault_path must not leak"
    );
    assert!(
        !serialized.contains("backup/primary"),
        "vault_path must not leak"
    );
    let dest_id = dest["item"]["id"].as_i64().unwrap();

    // Job: created (defaults applied), listed by project, fetched.
    let resp = create_job(address, token, project_id, dest_id as i32, "nightly").await;
    assert_eq!(resp.status().as_u16(), 201);
    let job = body_of(resp).await;
    assert_eq!(job["item"]["source"], "postgres");
    assert_eq!(job["item"]["retentionDaily"], 7);
    assert_eq!(job["item"]["retentionWeekly"], 4);
    let job_id = job["item"]["id"].as_i64().unwrap();

    let resp = reqwest::Client::new()
        .get(format!(
            "{address}/api/v1/backups/jobs?projectId={project_id}"
        ))
        .header("Authorization", auth(token))
        .send()
        .await
        .expect("list jobs");
    assert_eq!(resp.status().as_u16(), 200);
    let list = body_of(resp).await;
    assert_eq!(list["list"].as_array().unwrap().len(), 1);

    // Update: only provided fields change.
    let resp = reqwest::Client::new()
        .patch(format!("{address}/api/v1/backups/jobs/{job_id}"))
        .header("Authorization", auth(token))
        .json(&json!({ "enabled": false, "retentionDaily": 3 }))
        .send()
        .await
        .expect("update job");
    assert_eq!(resp.status().as_u16(), 200);
    let updated = body_of(resp).await;
    assert_eq!(updated["item"]["enabled"], false);
    assert_eq!(updated["item"]["retentionDaily"], 3);
    assert_eq!(updated["item"]["target"], "shop-postgres-1", "untouched");

    // Runs list is empty but reachable.
    let resp = reqwest::Client::new()
        .get(format!("{address}/api/v1/backups/jobs/{job_id}/runs"))
        .header("Authorization", auth(token))
        .send()
        .await
        .expect("list runs");
    assert_eq!(resp.status().as_u16(), 200);
    assert!(body_of(resp).await["list"].as_array().unwrap().is_empty());

    // Validation: bad source is rejected.
    let resp = reqwest::Client::new()
        .post(format!("{address}/api/v1/backups/jobs"))
        .header("Authorization", auth(token))
        .json(&json!({
            "projectId": project_id, "destinationId": dest_id,
            "name": "bad", "source": "oracle", "target": "x", "schedule": "0 3 * * *"
        }))
        .send()
        .await
        .expect("bad job");
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn another_user_cannot_touch_a_backup_job_or_destination() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let address = &app.address;
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;

    let resp = create_destination(
        address,
        common::USER_A_TOKEN,
        "primary",
        "https://b.example",
    )
    .await;
    let dest_id = body_of(resp).await["item"]["id"].as_i64().unwrap();
    let resp = create_job(
        address,
        common::USER_A_TOKEN,
        project_id,
        dest_id as i32,
        "nightly",
    )
    .await;
    let job_id = body_of(resp).await["item"]["id"].as_i64().unwrap();

    // User B (not the owner) gets 404 — the same answer as a missing resource.
    for (method, url) in [
        ("GET", format!("{address}/api/v1/backups/jobs/{job_id}")),
        ("DELETE", format!("{address}/api/v1/backups/jobs/{job_id}")),
        (
            "GET",
            format!("{address}/api/v1/backups/jobs/{job_id}/runs"),
        ),
        (
            "DELETE",
            format!("{address}/api/v1/backups/destinations/{dest_id}"),
        ),
    ] {
        let resp = reqwest::Client::new()
            .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), url)
            .header("Authorization", auth(common::USER_B_TOKEN))
            .send()
            .await
            .expect("cross-user request");
        assert_eq!(
            resp.status().as_u16(),
            404,
            "{method} must answer 404 for another user"
        );
    }
}
