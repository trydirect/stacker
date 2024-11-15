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

    let app = common::spawn_app().await; // server
    let client = reqwest::Client::new(); // client

    let response = client
        .get(&format!("{}/agreement/1", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    println!("response: {:?}", response);
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}


// test me: cargo t --test agreement user_add -- --nocapture --show-output
#[tokio::test]
async fn user_add() {

    let app = common::spawn_app().await; // server
    let client = reqwest::Client::new(); // client

    let data = r#"
    {
        "agrt_id": "1",
    }
    "#;

    let response = client
        .post(&format!("{}/agreement", &app.address))
        .json(data)
        .send()
        .await
        .expect("Failed to execute request.");

    println!("response: {}", response.status());
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
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
