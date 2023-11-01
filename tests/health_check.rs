//#[actix_rt::test]

use std::net::TcpListener;
use actix_web::{App, HttpServer, web, Responder, get};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use stacker::configuration::{get_configuration, DatabaseSettings};
use stacker::forms;


#[get("")]
async fn mock_auth() -> actix_web::Result<impl Responder> {
    println!("Starting auth server in test mode ...");
    // 1. set user id
    // 2. add token to header / hardcoded
    Ok(web::Json(forms::user::UserForm::default()))
}

async fn mock_auth_server(listener:TcpListener) -> actix_web::dev::Server {

    HttpServer::new(|| {
        App::new()
            .service(web::scope("/me")
                .service(mock_auth))
        })
        .listen(listener)
        .unwrap()
        .run()
}

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

    // let mut rt = tokio::runtime::Runtime::new().unwrap();
    // rt.spawn(mock_auth_server(listener)).expect("Could not spawn auth server");
    let mut configuration = get_configuration().expect("Failed to get configuration");

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind port for testing auth server");

    configuration.auth_url = format!("http://127.0.0.1:{}/me", listener.local_addr().unwrap().port());
    println!("Auth Server is running on: {}", configuration.auth_url);

    let handle = tokio::spawn(mock_auth_server(listener));
    handle.await.expect("Auth Server can not be started");

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind random port");

    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);
    configuration.database.database_name = uuid::Uuid::new_v4().to_string();

    let connection_pool = configure_database(&configuration.database).await;

    let server = stacker::startup::run(listener, connection_pool.clone(), configuration)
        .await.expect("Failed to bind address.");

    let _ = tokio::spawn(server);
    println!("Used Port: {}", port);

    TestApp {
        address,
        db_pool: connection_pool,
    }
}

#[tokio::test]
async fn add_rating_returns_a_200_for_valid_form_data() {
    // Arrange
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    // let body = "name=le%20guin&email=ursula_le_guin%40gmail.com"; // %20 - space, %40 - @
    // let response = client
    //     .post(&format!("{}/subscriptions", &app.address))
    //     .header("Content-Type", "application/x-www-form-urlencoded")
    //     .body(body)
    //     .send()
    //     .await
    //     .expect("Failed to execute request.");
    //
    // assert_eq!(200, response.status().as_u16());
    //
    // let saved = sqlx::query!("SELECT email, name FROM subscriptions",)
    //     .fetch_one(&app.db_pool)
    //     .await
    //     .expect("Failed to fetch saved subscription.");
    //
    // assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    // assert_eq!(saved.name, "le guin");
}