mod common;

// test me: cargo t --test cloud -- --nocapture --show-output
#[tokio::test]
async fn list() {
    let app = common::spawn_app().await; // server
    let client = reqwest::Client::new(); // client

    let response = client
        .get(&format!("{}/cloud", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

// test me: cargo t --test cloud add_cloud -- --nocapture --show-output
#[tokio::test]
async fn add_cloud() {
    let app = common::spawn_app().await; // server
    let client = reqwest::Client::new(); // client

    let data = r#"
    {
        "user_id": "fake_user_id",
        "provider": "htz",
        "cloud_token": "",
        "cloud_key": "",
        "cloud_secret": "",
        "save_token": true
    }
    "#;

    let response = client
        .post(&format!("{}/cloud", &app.address))
        .json(data)
        .send()
        .await
        .expect("Failed to execute request.");

    println!("response: {}", response.status());
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}
