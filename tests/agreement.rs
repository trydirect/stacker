mod common;
// test me:
// cargo t --test agreement -- --nocapture --show-output

// test specific function: cargo t --test agreement admin_add -- --nocapture --show-output
// #[tokio::test]
// async fn admin_add() {
//
//     let app = common::spawn_app().await; // server
//     let client = reqwest::Client::new(); // client
//
//     let data = r#"
//     {
//         "name": "test",
//         "text": "test agreement text
//     }
//     "#;
//
//     let response = client
//         .post(&format!("{}/admin/agreement", &app.address))
//         .json(data)
//         .send()
//         .await
//         .expect("Failed to execute request.");
//
//     println!("response: {}", response.status());
//     assert!(response.status().is_success());
//     assert_eq!(Some(0), response.content_length());
// }
//
// test me: cargo t --test agreement admin_fetch_one -- --nocapture --show-output
// #[tokio::test]
// async fn admin_fetch_one() {
//
//     let app = common::spawn_app().await; // server
//     let client = reqwest::Client::new(); // client
//
//     let response = client
//         .get(&format!("{}/admin/agreement/1", &app.address))
//         .send()
//         .await
//         .expect("Failed to execute request.");
//
//     assert!(response.status().is_success());
//     assert_eq!(Some(0), response.content_length());
// }
//
// test me: cargo t --test agreement get --nocapture --show-output
#[tokio::test]
async fn get() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    }; // server
    let client = reqwest::Client::new(); // client

    // Pre-insert an agreement so the handler has something to return
    let agreement_id: i32 = sqlx::query_scalar(
        "INSERT INTO agreement (name, text, created_at, updated_at) \
         VALUES ('Test Agreement', 'Test text', NOW(), NOW()) RETURNING id",
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to insert test agreement");

    let response = client
        .get(&format!("{}/agreement/{}", &app.address, agreement_id))
        .header("Authorization", "Bearer test_token")
        .send()
        .await
        .expect("Failed to execute request.");

    println!("response: {:?}", response);
    assert!(response.status().is_success());
}

// test me: cargo t --test agreement user_add -- --nocapture --show-output
#[tokio::test]
async fn user_add() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    }; // server
    let client = reqwest::Client::new(); // client

    // Pre-insert an agreement that the user will accept
    let agreement_id: i32 = sqlx::query_scalar(
        "INSERT INTO agreement (name, text, created_at, updated_at) \
         VALUES ('Test Agreement', 'Test text', NOW(), NOW()) RETURNING id",
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to insert test agreement");

    let data = serde_json::json!({ "agrt_id": agreement_id });

    let response = client
        .post(&format!("{}/agreement", &app.address))
        .header("Authorization", "Bearer test_token")
        .json(&data)
        .send()
        .await
        .expect("Failed to execute request.");

    println!("response: {}", response.status());
    assert!(response.status().is_success());
}

#[tokio::test]
async fn user_add_via_api_prefix() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let agreement_id: i32 = sqlx::query_scalar(
        "INSERT INTO agreement (name, text, created_at, updated_at) \
         VALUES ('API Agreement', 'Test text', NOW(), NOW()) RETURNING id",
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to insert test agreement");

    let data = serde_json::json!({ "agrt_id": agreement_id });

    let response = client
        .post(&format!("{}/api/agreement", &app.address))
        .header("Authorization", "Bearer test_token")
        .json(&data)
        .send()
        .await
        .expect("Failed to execute request.");

    println!("response: {}", response.status());
    assert!(response.status().is_success());
}

// // test me: cargo t --test agreement admin_update -- --nocapture --show-output
// #[tokio::test]
// async fn admin_update() {
//
//     let app = common::spawn_app().await; // server
//     let client = reqwest::Client::new(); // client
//
//     let data = r#"
//     {
//         "name": "test update",
//         "text": "test agreement text update
//     }
//     "#;
//
//     let response = client
//         .post(&format!("{}/admin/agreement", &app.address))
//         .json(data)
//         .send()
//         .await
//         .expect("Failed to execute request.");
//
//     println!("response: {}", response.status());
//     assert!(response.status().is_success());
//     assert_eq!(Some(0), response.content_length());
// }
//
