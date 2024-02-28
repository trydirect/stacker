use stacker::forms::stack::StackForm;
use stacker::forms::stack::DockerImage;
use stacker::forms::stack::App;
use std::fs;
use std::collections::HashMap;

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
#[test]
fn test_deserialize_user_stack() {

    let body_str = fs::read_to_string("./tests/custom-stack-payload-11.json").unwrap();
    let form = serde_json::from_str::<StackForm>(&body_str).unwrap();
    println!("{:?}", form);
    // @todo assert required data

    // {
    //     Ok(f) => {
    //         f
    //     }
    //     Err(_err) => {
    //         let msg = format!("Invalid data. {:?}", _err);
    //         return JsonResponse::<StackForm>::build().bad_request(msg);
    //     }
    // };
    //
    // assert_eq!(result, 12);

    // let form:Environment = serde_json::from_str(&body_str).unwrap();

    // let body_str = r#"
    // [
    // {
    //   "ENV_VAR1": "ENV_VAR1_VALUE"
    // },
    // {
    //   "ENV_VAR2": "ENV_VAR2_VALUE",
    //   "ENV_VAR3": "ENV_VAR3_VALUE"
    // }
    // ]
    // "#;
    // let form:Vec<HashMap<String, String>> = serde_json::from_str(&body_str).unwrap();
    // println!("{:?}", form);
}

#[test]
fn test_docker_image_only_name_other_empty() {
    let docker_image = DockerImage {
        dockerhub_user: Some("".to_string()),
        dockerhub_name: Some("mysql".to_string()),
        dockerhub_image: Some("".to_string(),),
        dockerhub_password: None,
    };
    let output = docker_image.to_string();
    assert_eq!(String::from("mysql:latest"), output);
}

#[test]
fn test_docker_image_only_name_other_none() {
    let docker_image = DockerImage {
        dockerhub_user: None,
        dockerhub_name: Some("mysql".to_string()),
        dockerhub_image: None,
        dockerhub_password: None,
    };
    let output = docker_image.to_string();
    assert_eq!(String::from("mysql:latest"), output);
}
#[test]
fn test_docker_image_namespace_and_repo() {
    let docker_image = DockerImage {
        dockerhub_user: Some("trydirect".to_string()),
        dockerhub_name: Some("mysql".to_string()),
        dockerhub_image: Some("".to_string(),),
        dockerhub_password: None,
    };
    let output = docker_image.to_string();
    assert_eq!(String::from("trydirect/mysql:latest"), output);
}

#[test]
fn test_docker_image_namespace_and_repo_tag() {
    let docker_image = DockerImage {
        dockerhub_user: Some("trydirect".to_string()),
        dockerhub_name: Some("mysql:8.1".to_string()),
        dockerhub_image: Some("".to_string(),),
        dockerhub_password: None,
    };
    let output = docker_image.to_string();
    assert_eq!(String::from("trydirect/mysql:8.1"), output);
}
#[test]
fn test_docker_image_only_image() {
    let docker_image = DockerImage {
        dockerhub_user: None,
        dockerhub_name: None,
        dockerhub_image: Some("trydirect/mysql:stable".to_string(),),
        dockerhub_password: None,
    };
    let output = docker_image.to_string();
    assert_eq!(String::from("trydirect/mysql:stable"), output);
}

#[test]
fn test_docker_image_only_image_other_empty() {
    let docker_image = DockerImage {
        dockerhub_user: Some("".to_string()),
        dockerhub_name: Some("".to_string()),
        dockerhub_image: Some("trydirect/mysql:stable".to_string()),
        dockerhub_password: None,
    };
    let output = docker_image.to_string();
    assert_eq!(String::from("trydirect/mysql:stable"), output);
}
#[test]
fn test_docker_repo_name_with_tag_other_none() {
    let docker_image = DockerImage {
        dockerhub_user: None,
        dockerhub_name: Some("mysql:stable".to_string()),
        dockerhub_image: None,
        dockerhub_password: None,
    };
    let output = docker_image.to_string();
    assert_eq!(String::from("mysql:stable"), output);
}
