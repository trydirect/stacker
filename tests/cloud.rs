mod common;

// test me: cargo t --test cloud -- --nocapture --show-output
#[tokio::test]
async fn list() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    }; // server
    let client = reqwest::Client::new(); // client

    let response = client
        .get(&format!("{}/cloud", &app.address))
        .header("Authorization", "Bearer test_token")
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
}

// test me: cargo t --test cloud add_cloud -- --nocapture --show-output
#[tokio::test]
async fn add_cloud() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    }; // server
    let client = reqwest::Client::new(); // client

    let data = serde_json::json!({
        "provider": "htz",
        "save_token": false
    });

    let response = client
        .post(&format!("{}/cloud", &app.address))
        .header("Authorization", "Bearer test_token")
        .json(&data)
        .send()
        .await
        .expect("Failed to execute request.");

    println!("response: {}", response.status());
    assert!(response.status().is_success());
}
