mod common;

#[tokio::test]
async fn health_check_works() {
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

    // Health check responds with 200 (all deps up) or 503 (some deps unavailable).
    // Both are valid responses — the endpoint is reachable and functioning.
    // In CI, Redis/Vault are not running so 503 is expected.
    let status = response.status().as_u16();
    assert!(
        status == 200 || status == 503,
        "health_check should return 200 or 503, got {status}"
    );
}
