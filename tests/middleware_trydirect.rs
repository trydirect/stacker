mod common;
use wiremock::MockServer;

#[tokio::test]
async fn middleware_trydirect_works() {
    // 1. Arrange
    let trydirect_auth_server = MockServer::start().await;

    // 2. Act
    // 3. Assert

    println!("Before spawn_app");
    let app = common::spawn_app().await; // server
    println!("After spawn_app");
    let client = reqwest::Client::new(); // client

    let response = client
        .get(&format!("{}/health_check", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());


    //todo header stacker-id not found
    //
}
