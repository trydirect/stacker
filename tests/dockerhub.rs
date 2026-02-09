// use std::fs;
// use std::collections::HashMap;
use docker_compose_types::ComposeVolume;

mod common;
use stacker::forms::project::DockerImage;
// use stacker::helpers::project::dctypes::{ComposeVolume, SingleValue};
use serde_yaml;
use stacker::forms::project::Volume;

const DOCKER_USERNAME: &str = "trydirect";
const DOCKER_PASSWORD: &str = "**********";
//  Unit Test

// #[test]
// fn test_deserialize_project_web() {
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
//     //         return JsonResponse::<ProjectForm>::build().bad_request(msg);
//     //     }
//     // };
//     //
//     // assert_eq!(result, 12);
// }
// #[test]
// fn test_deserialize_project() {
//
//     let body_str = fs::read_to_string("./tests/custom-project-payload-11.json").unwrap();
//     let form = serde_json::from_str::<ProjectForm>(&body_str).unwrap();
//     println!("{:?}", form);
//     // @todo assert required data
//
//     // {
//     //     Ok(f) => {
//     //         f
//     //     }
//     //     Err(_err) => {
//     //         let msg = format!("Invalid data. {:?}", _err);
//     //         return JsonResponse::<ProjectForm>::build().bad_request(msg);
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
async fn test_docker_hub_successful_login() {
    if common::spawn_app().await.is_none() {
        return;
    } // server
      // let username = env::var("TEST_DOCKER_USERNAME")
      //     .expect("username environment variable is not set");
      //
      // let password= env::var("TEST_DOCKER_PASSWORD")
      //     .expect("password environment variable is not set");
    let di = DockerImage {
        dockerhub_user: Some(String::from("trydirect")),
        dockerhub_name: Some(String::from("nginx-waf")),
        dockerhub_image: None,
        dockerhub_password: Some(String::from(DOCKER_PASSWORD)),
    };
    assert_eq!(di.is_active().await.unwrap(), true);
}

#[tokio::test]
async fn test_docker_private_exists() {
    if common::spawn_app().await.is_none() {
        return;
    } // server
    let di = DockerImage {
        dockerhub_user: Some(String::from("trydirect")),
        dockerhub_name: Some(String::from("nginx-waf")),
        dockerhub_image: None,
        dockerhub_password: Some(String::from(DOCKER_PASSWORD)),
    };
    assert_eq!(di.is_active().await.unwrap(), true);
}

#[tokio::test]
async fn test_public_repo_is_accessible() {
    if common::spawn_app().await.is_none() {
        return;
    } // server
    let di = DockerImage {
        dockerhub_user: Some(String::from("")),
        dockerhub_name: Some(String::from("nginx")),
        dockerhub_image: None,
        dockerhub_password: Some(String::from("")),
    };
    assert_eq!(di.is_active().await.unwrap(), true);
}
#[tokio::test]
async fn test_docker_non_existent_repo() {
    if common::spawn_app().await.is_none() {
        return;
    } // server
    let di = DockerImage {
        dockerhub_user: Some(String::from("trydirect")), //namespace
        dockerhub_name: Some(String::from("nonexistent")), //repo
        dockerhub_image: None, // namesps/reponame:tag full docker image string
        dockerhub_password: Some(String::from("")),
    };
    println!("{}", di.is_active().await.unwrap());
    assert_eq!(di.is_active().await.unwrap(), false);
}

#[tokio::test]
async fn test_docker_non_existent_repo_empty_namespace() {
    if common::spawn_app().await.is_none() {
        return;
    } // server
    let di = DockerImage {
        dockerhub_user: Some(String::from("")),            //namespace
        dockerhub_name: Some(String::from("nonexistent")), //repo
        dockerhub_image: None, // namesps/reponame:tag full docker image string
        dockerhub_password: Some(String::from("")),
    };
    assert_eq!(di.is_active().await.unwrap(), true);
}

#[tokio::test]
async fn test_docker_named_volume() {
    let volume = Volume {
        host_path: Some("flask-data".to_owned()),
        container_path: Some("/var/www/flaskdata".to_owned()),
    };

    let cv: ComposeVolume = (&volume).into();
    println!("ComposeVolume: {:?}", cv);
    println!("{:?}", cv.driver_opts);
    assert_eq!(Some("flask-data".to_string()), cv.name);
    assert!(cv.driver.is_none());
    assert!(cv.driver_opts.is_empty());
}
