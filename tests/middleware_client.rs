mod common;

#[tokio::test]
async fn middleware_client_works() {
    // 1. Arrange
    // 2. Act
    // 3. Assert

    println!("Before spawn_app");
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    }; // server
    println!("After spawn_app");
    let client = reqwest::Client::new(); // client

    let response = client
        .get(&format!("{}/health_check", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
    assert!(
        response.content_length().unwrap_or(0) > 0,
        "health_check should return a non-empty JSON body"
    );

    //todo header stacker-id not found
    //
}
