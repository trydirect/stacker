// use std::fs;
// use std::collections::HashMap;
use std::env;
mod common;
use stacker::helpers::dockerhub::{login, docker_image_exists};

const DOCKER_USERNAME: &str = "trydirect";
const DOCKER_PASSWORD: &str = "***REMOVED***";

//  Unit Test

// #[test]
// fn test_deserialize_user_stack_web() {
//
//     let body_str = fs::read_to_string("./tests/web-item.json").unwrap();
//     // let form:serde_json::Value = serde_json::from_str(&body_str).unwrap();
//     let form:App = serde_json::from_str(&body_str).unwrap();
//     println!("{:?}", form);
//     // {
//     //     Ok(f) => {
//     //         f
//     //     }
//     //     Err(_err) => {
//     //         let msg = format!("Invalid data. {:?}", _err);
//     //         return JsonResponse::<StackForm>::build().bad_request(msg);
//     //     }
//     // };
//     //
//     // assert_eq!(result, 12);
// }
// #[test]
// fn test_deserialize_user_stack() {
//
//     let body_str = fs::read_to_string("./tests/custom-stack-payload-11.json").unwrap();
//     let form = serde_json::from_str::<StackForm>(&body_str).unwrap();
//     println!("{:?}", form);
//     // @todo assert required data
//
//     // {
//     //     Ok(f) => {
//     //         f
//     //     }
//     //     Err(_err) => {
//     //         let msg = format!("Invalid data. {:?}", _err);
//     //         return JsonResponse::<StackForm>::build().bad_request(msg);
//     //     }
//     // };
//     //
//     // assert_eq!(result, 12);
//
//     // let form:Environment = serde_json::from_str(&body_str).unwrap();
//     // let form:Vec<HashMap<String, String>> = serde_json::from_str(&body_str).unwrap();
//     // println!("{:?}", form);
// }

#[tokio::test]
async fn test_docker_hub_login() {

    common::spawn_app().await; // server
    // let username = env::var("TEST_DOCKER_USERNAME")
    //     .expect("username environment variable is not set");
    //
    // let password= env::var("TEST_DOCKER_PASSWORD")
    //     .expect("password environment variable is not set");
    let result = login(
        DOCKER_USERNAME.as_ref(),
        DOCKER_PASSWORD.as_ref()
    ).await;
    let token = result.unwrap().token;

    let exists: bool = match token {
        Some(tok) => {
            let exists = docker_image_exists(DOCKER_USERNAME, "nginx", tok).await;
            match exists {
                Ok(_) => true,
                Err(err) => {
                    println!("{:?}", err);
                    false
                }
            }
        }
        _ => false,
    };

    assert_eq!(exists, true);
}

#[tokio::test]
async fn test_docker_private_exists() {

    common::spawn_app().await; // server
    let result = login(
        DOCKER_USERNAME.as_ref(), DOCKER_PASSWORD.as_ref()
    ).await;
    let token = result.unwrap().token;

    let exists: bool = match token {
        Some(tok) => {
            let exists = docker_image_exists(DOCKER_USERNAME, "nginx-waf", tok).await;
            match exists {
                Ok(_) => true,
                Err(err) => {
                    println!("{:?}", err);
                    false
                }
            }
        }
        _ => false,
    };

    assert_eq!(exists, true);
}

#[tokio::test]
async fn test_docker_non_existent_repo() {

    common::spawn_app().await; // server
    let result= login(
        DOCKER_USERNAME.as_ref(), DOCKER_PASSWORD.as_ref()
    ).await;
    let token = result.unwrap().token;

    // println!("{:?}", token);
    let exists: bool = match token {
        Some(tok) => {
            let exists = docker_image_exists(DOCKER_USERNAME, "nonexistent", tok).await;
            match exists {
                Ok(_) => true,
                Err(err) => {
                    println!("{:?}", err);
                    false
                }
            }
        }
        _ => false,
    };

    assert_eq!(exists, false);
}
