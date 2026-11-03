//! Quick Deploy provisions the server type a stack was baked on, at that
//! type's price. The contract is mirrored from
//! ../config/shared-fixtures/api-contracts/quick-deploy-server-type.json to
//! tests/contracts/quick-deploy-server-type.contract.json.

mod common;

use serde_json::Value;

fn contract() -> Value {
    serde_json::from_str(include_str!(
        "contracts/quick-deploy-server-type.contract.json"
    ))
    .expect("contract JSON")
}

async fn validate(app: &common::TestApp, stack: &str) -> Value {
    let yml = format!("name: {stack}\napp:\n  type: custom\n  image: nginx:1.27\n");
    let resp = reqwest::Client::new()
        .post(format!("{}/api/v1/deploy/validate", app.address))
        .header("Content-Type", "text/plain")
        .body(yml)
        .send()
        .await
        .expect("validate request");
    assert_eq!(resp.status().as_u16(), 200);
    resp.json().await.expect("validate JSON")
}

#[tokio::test]
async fn validate_names_the_snapshot_server_type_and_lists_the_rates() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let contract = contract();
    stacker::db::baked_snapshot::record(
        &app.db_pool,
        "qd-baked-on-cpx42",
        "1.0.0",
        "hetzner",
        444002406,
        true,
        None,
        None,
        None,
        Some("cpx42"),
    )
    .await
    .expect("record snapshot");

    let body = validate(&app, "qd-baked-on-cpx42").await;
    let quick = &body["quick_deploy"];
    assert_eq!(quick["server_type"], "cpx42", "{body}");
    for (server_type, rate) in contract["rates"].as_object().unwrap() {
        assert_eq!(
            quick["rates"][server_type]["daily_rate"].as_f64(),
            rate["daily_rate"].as_f64(),
            "{server_type}: {quick}"
        );
        assert_eq!(
            quick["rates"][server_type]["monthly_cap"].as_f64(),
            rate["monthly_cap"].as_f64(),
            "{server_type}: {quick}"
        );
    }

    // A stacker.yml that pins a version gets that version's snapshot - the
    // one the clone uses - not the latest.
    stacker::db::baked_snapshot::record(
        &app.db_pool,
        "qd-baked-on-cpx42",
        "0.9.0",
        "hetzner",
        1,
        true,
        None,
        None,
        None,
        Some("cpx32"),
    )
    .await
    .expect("record older snapshot");
    let yml =
        "name: qd-baked-on-cpx42\nversion: \"0.9.0\"\napp:\n  type: custom\n  image: nginx:1.27\n";
    let body: Value = reqwest::Client::new()
        .post(format!("{}/api/v1/deploy/validate", app.address))
        .body(yml)
        .send()
        .await
        .expect("validate request")
        .json()
        .await
        .expect("validate JSON");
    assert_eq!(body["quick_deploy"]["server_type"], "cpx32", "{body}");

    // No snapshot - or one baked before the type was recorded - names none,
    // and the caller's default type applies.
    let body = validate(&app, "qd-never-baked").await;
    assert!(body["quick_deploy"]["server_type"].is_null(), "{body}");
    assert!(
        body["quick_deploy"]["rates"].get("cpx32").is_some(),
        "{body}"
    );
}

#[tokio::test]
async fn a_recorded_snapshot_keeps_its_server_type() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let row = stacker::db::baked_snapshot::record(
        &app.db_pool,
        "qd-record",
        "1.0.0",
        "hetzner",
        1,
        true,
        None,
        None,
        None,
        Some("cpx42"),
    )
    .await
    .expect("record snapshot");
    assert_eq!(row.server_type.as_deref(), Some("cpx42"));
}
