//#[actix_rt::test]

use sqlx::{Connection, Executor, PgConnection, PgPool};
use stacker::configuration::{get_configuration, DatabaseSettings};

#[tokio::test]
async fn health_check_works() {
    // 1. Arrange
    // 2. Act
    // 3. Assert

    println!("Before spawn_app");
    let app = spawn_app().await; // server
    println!("After spawn_app");
    let client = reqwest::Client::new(); // client

    let response = client
        .get(&format!("{}/health_check", &app.address))
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
    // let app = App::new().service(web::resource("/health_check").route(web::get().to(health_check)));
    // let mut app = test::init_service(app).await;
    // let req = test::TestRequest::get().uri("/health_check").to_request();
    // let resp = test::call_service(&mut app, req).await;
    // assert_eq!(resp.status(), StatusCode::OK);
}

// test that locks main thread
// async fn spawn_app() -> std::io::Result<()> {
//     stacker::run().await
// }

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}

pub async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect(&config.connection_string_without_db())
        .await
        .expect("Failed to connect to postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}""#, config.database_name).as_str())
        .await
        .expect("Failed to create database");

    let connection_pool = PgPool::connect(&config.connection_string())
        .await
        .expect("Failed to connect to database pool");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate database");

    connection_pool
}

// we have to run server in another thread
async fn spawn_app() -> TestApp {
    // Future<TestApp>
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");

    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);
    let mut configuration = get_configuration().expect("Failed to get configuration");
    configuration.database.database_name = uuid::Uuid::new_v4().to_string();

    let connection_pool = configure_database(&configuration.database).await;
    //let connection_pool = PgPool::connect(&configuration.database.connection_string())
    //.await
    //.expect("Failed to connect to database");

    let server = stacker::startup::run(listener, connection_pool.clone())
        .expect("Failed to bind address.");

    let _ = tokio::spawn(server);
    println!("Used Port: {}", port);
    //format!("http://127.0.0.1:{}", port)
    TestApp {
        address,
        db_pool: connection_pool,
    }
}
