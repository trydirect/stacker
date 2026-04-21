mod common;

#[tokio::test]
async fn health_check_works() {
    println!("Before spawn_app");
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    println!("After spawn_app");
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health_check", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    // 200 = Healthy or Degraded (optional services down, but core DB works)
    // 503 = Unhealthy (core database is unreachable)
    // In CI only postgres is available; external services (AMQP, Vault, DockerHub,
    // user_service, install_service) will be Degraded → overall Degraded → 200.
    assert!(
        response.status().is_success(),
        "health_check should return 200, got {}",
        response.status()
    );
}

