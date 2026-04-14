use cucumber::{given, then, when};
use serde_json::json;

use super::StepWorld;

// ─── Auth steps ──────────────────────────────────────────────────

#[given("I am authenticated as User A")]
async fn auth_as_user_a(world: &mut StepWorld) {
    world.auth_token = "user-a-token".to_string();
}

#[when("I switch to User B")]
async fn switch_to_user_b(world: &mut StepWorld) {
    world.auth_token = "user-b-token".to_string();
}

// ─── Project CRUD steps ─────────────────────────────────────────

#[when(regex = r#"^I create a project with stack code "(.+)"$"#)]
async fn create_project(world: &mut StepWorld, stack_code: String) {
    let body = json!({
        "custom": {
            "custom_stack_code": stack_code,
            "web": [{
                "_id": "bdd-app-1",
                "code": "nginx",
                "name": "Nginx",
                "type": "web",
                "restart": "always",
                "dockerhub_name": "nginx",
                "custom": true
            }]
        }
    });
    world.post_json("/project", &body).await;
    if world.status_code == Some(200) {
        world.store_id_from_response("project_id", "/item/id");
    }
}

#[given(regex = r#"^I have created a project with stack code "(.+)"$"#)]
async fn given_project_created(world: &mut StepWorld, stack_code: String) {
    let body = json!({
        "custom": {
            "custom_stack_code": stack_code,
            "web": [{
                "_id": "bdd-app-1",
                "code": "nginx",
                "name": "Nginx",
                "type": "web",
                "restart": "always",
                "dockerhub_name": "nginx",
                "custom": true
            }]
        }
    });
    world.post_json("/project", &body).await;
    assert_eq!(
        world.status_code,
        Some(200),
        "Failed to create project '{}': {}",
        stack_code,
        world.response_body.as_deref().unwrap_or("<none>")
    );
    world.store_id_from_response("project_id", "/item/id");
}

#[when(regex = r#"^I send a GET request to the stored "(.+)" at "(.+)"$"#)]
async fn get_with_stored_id(world: &mut StepWorld, id_key: String, path_template: String) {
    let id = world
        .stored_ids
        .get(&id_key)
        .unwrap_or_else(|| panic!("No stored ID for '{}'", id_key))
        .clone();
    let path = path_template.replace("{id}", &id);
    world.get(&path).await;
}

#[when(regex = r#"^I update the stored project with stack code "(.+)"$"#)]
async fn update_project(world: &mut StepWorld, stack_code: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    let body = json!({
        "custom": {
            "custom_stack_code": stack_code,
            "web": [{
                "_id": "bdd-app-1",
                "code": "nginx",
                "name": "Nginx",
                "type": "web",
                "restart": "always",
                "dockerhub_name": "nginx",
                "custom": true
            }]
        }
    });
    world.put_json(&format!("/project/{}", id), &body).await;
}

#[when("I delete the stored project")]
async fn delete_project(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    world.delete(&format!("/project/{}", id)).await;
}

#[then(regex = r#"^I store the response JSON "(.+)" as "(.+)"$"#)]
async fn store_json_value(world: &mut StepWorld, json_path: String, key: String) {
    world.store_id_from_response(&key, &json_path);
    assert!(
        world.stored_ids.contains_key(&key),
        "Failed to store '{}' from JSON path '{}'",
        key,
        json_path
    );
}

#[then(regex = r#"^the response JSON list should have at least (\d+) items$"#)]
async fn check_list_min_items(world: &mut StepWorld, min_count: usize) {
    let json = world
        .response_json
        .as_ref()
        .expect("No JSON response available");
    let list = json
        .get("list")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("No 'list' array in response: {}", json));
    assert!(
        list.len() >= min_count,
        "Expected at least {} items, got {}",
        min_count,
        list.len()
    );
}

#[then(regex = r#"^the response JSON list should not contain project "(.+)"$"#)]
async fn check_list_not_contains(world: &mut StepWorld, name: String) {
    let json = world
        .response_json
        .as_ref()
        .expect("No JSON response available");
    let list = json
        .get("list")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("No 'list' array in response: {}", json));
    let found = list
        .iter()
        .any(|item| item.get("name").and_then(|n| n.as_str()) == Some(&name));
    assert!(!found, "List should NOT contain project '{}'", name);
}

// ─── Team / Member steps ─────────────────────────────────────────

#[when(regex = r#"^I add member "(.+)" with role "(.+)" to the stored project$"#)]
#[given(regex = r#"^I add member "(.+)" with role "(.+)" to the stored project$"#)]
async fn add_member(world: &mut StepWorld, user_id: String, role: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    let body = json!({ "user_id": user_id, "role": role });
    world
        .post_json(&format!("/project/{}/members", id), &body)
        .await;
}

#[when("I list members of the stored project")]
async fn list_members(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    world.get(&format!("/project/{}/members", id)).await;
}

#[when(regex = r#"^I remove member "(.+)" from the stored project$"#)]
async fn remove_member(world: &mut StepWorld, member_id: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    world
        .delete(&format!("/project/{}/members/{}", id, member_id))
        .await;
}

// ─── App steps ───────────────────────────────────────────────────

#[when(regex = r#"^I create an app "(.*)" with image "(.*)" in the stored project$"#)]
#[given(regex = r#"^I have created an app "(.*)" with image "(.*)" in the stored project$"#)]
async fn create_app(world: &mut StepWorld, code: String, image: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    let body = json!({
        "code": code,
        "name": code,
        "image": image
    });
    world
        .post_json(&format!("/project/{}/apps", id), &body)
        .await;
}

#[when("I list apps in the stored project")]
async fn list_apps(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    world.get(&format!("/project/{}/apps", id)).await;
}

#[when(regex = r#"^I get app "(.+)" in the stored project$"#)]
async fn get_app(world: &mut StepWorld, code: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    world.get(&format!("/project/{}/apps/{}", id, code)).await;
}

#[when(regex = r#"^I get app config for "(.+)" in the stored project$"#)]
async fn get_app_config(world: &mut StepWorld, code: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    world
        .get(&format!("/project/{}/apps/{}/config", id, code))
        .await;
}

#[when(regex = r#"^I update env vars for app "(.*)" in the stored project with:$"#)]
async fn update_env_vars(world: &mut StepWorld, step: &cucumber::gherkin::Step, code: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    let table = step.table.as_ref().expect("table not found");
    let mut vars = serde_json::Map::new();
    // Skip header row (row 0), process data rows
    for row in table.rows.iter().skip(1) {
        if row.len() >= 2 {
            vars.insert(
                row[0].clone(),
                serde_json::Value::String(row[1].clone()),
            );
        }
    }
    let body = json!({ "variables": vars });
    world
        .put_json(&format!("/project/{}/apps/{}/env", id, code), &body)
        .await;
}

#[given(regex = r#"^I have set env var "(.+)" to "(.+)" for app "(.+)"$"#)]
async fn set_env_var(world: &mut StepWorld, key: String, value: String, code: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    let mut vars = serde_json::Map::new();
    vars.insert(key, serde_json::Value::String(value));
    let body = json!({ "variables": vars });
    world
        .put_json(&format!("/project/{}/apps/{}/env", id, code), &body)
        .await;
    assert!(
        world.status_code == Some(200),
        "Failed to set env var: {}",
        world.response_body.as_deref().unwrap_or("<none>")
    );
}

#[when(regex = r#"^I get env vars for app "(.+)" in the stored project$"#)]
async fn get_env_vars(world: &mut StepWorld, code: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    world
        .get(&format!("/project/{}/apps/{}/env", id, code))
        .await;
}

#[when(regex = r#"^I delete env var "(.+)" for app "(.+)" in the stored project$"#)]
async fn delete_env_var(world: &mut StepWorld, var_name: String, code: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    world
        .delete(&format!("/project/{}/apps/{}/env/{}", id, code, var_name))
        .await;
}

#[when(regex = r#"^I update ports for app "(.+)" in the stored project with host (\d+) container (\d+)$"#)]
async fn update_ports(world: &mut StepWorld, code: String, host: u16, container: u16) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    let body = json!({
        "ports": [{ "host": host, "container": container, "protocol": "tcp" }]
    });
    world
        .put_json(&format!("/project/{}/apps/{}/ports", id, code), &body)
        .await;
}

#[when(regex = r#"^I update domain for app "(.+)" in the stored project to "(.+)" with SSL$"#)]
async fn update_domain_ssl(world: &mut StepWorld, code: String, domain: String) {
    let id = world
        .stored_ids
        .get("project_id")
        .expect("No stored project_id")
        .clone();
    let body = json!({ "domain": domain, "ssl_enabled": true });
    world
        .put_json(&format!("/project/{}/apps/{}/domain", id, code), &body)
        .await;
}
