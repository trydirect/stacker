use cucumber::given;
use cucumber::then;
use cucumber::when;
use serde_json::json;

use crate::steps::StepWorld;

// ─── Client steps ───

fn store_client_id(world: &mut StepWorld) {
    if let Some(json) = &world.response_json {
        if let Some(id) = json.pointer("/item/id").and_then(|v| v.as_i64()) {
            world
                .stored_ids
                .insert("client_id".to_string(), id.to_string());
        }
    }
}

#[when("I create a client")]
async fn create_client(world: &mut StepWorld) {
    world.post_json("/client", &json!({})).await;
    store_client_id(world);
}

#[given("I have created a client")]
async fn given_created_client(world: &mut StepWorld) {
    world.post_json("/client", &json!({})).await;
    store_client_id(world);
}

#[when("I disable the stored client")]
async fn disable_client(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("client_id")
        .expect("No stored client_id")
        .clone();
    world
        .put_json(&format!("/client/{}/disable", id), &json!({}))
        .await;
}

#[given("I disable the stored client")]
async fn given_disable_client(world: &mut StepWorld) {
    disable_client(world).await;
}

#[when("I enable the stored client")]
async fn enable_client(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("client_id")
        .expect("No stored client_id")
        .clone();
    world
        .put_json(&format!("/client/{}/enable", id), &json!({}))
        .await;
}

#[when("I update the stored client")]
async fn update_client(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("client_id")
        .expect("No stored client_id")
        .clone();
    world.put_json(&format!("/client/{}", id), &json!({})).await;
}

// ─── Rating steps ───

fn store_rating_id(world: &mut StepWorld) {
    if let Some(json) = &world.response_json {
        if let Some(id) = json.pointer("/item/id").and_then(|v| v.as_i64()) {
            world
                .stored_ids
                .insert("rating_id".to_string(), id.to_string());
        }
    }
}

#[when(
    regex = r#"^I create a rating for object (\d+) category "([^"]+)" with rate (\d+) and comment "([^"]*)"$"#
)]
async fn create_rating_with_comment(
    world: &mut StepWorld,
    obj_id: i32,
    category: String,
    rate: i32,
    comment: String,
) {
    // Ensure a product row exists for this obj_id
    let pool = world.db_pool.as_ref().unwrap();
    let _ = sqlx::query(
        "INSERT INTO product (id, obj_id, obj_type, created_at, updated_at) \
         VALUES ($1, $1, 'project', NOW(), NOW()) ON CONFLICT (id) DO NOTHING",
    )
    .bind(obj_id)
    .execute(pool)
    .await;

    let body = json!({
        "obj_id": obj_id,
        "category": category,
        "rate": rate,
        "comment": comment
    });
    world.post_json("/rating", &body).await;
    store_rating_id(world);
}

#[given(regex = r#"^I have created a rating for object (\d+) category "([^"]+)" with rate (\d+)$"#)]
async fn given_created_rating(world: &mut StepWorld, obj_id: i32, category: String, rate: i32) {
    // Ensure a product row exists for this obj_id
    let pool = world.db_pool.as_ref().unwrap();
    let _ = sqlx::query(
        "INSERT INTO product (id, obj_id, obj_type, created_at, updated_at) \
         VALUES ($1, $1, 'project', NOW(), NOW()) ON CONFLICT (id) DO NOTHING",
    )
    .bind(obj_id)
    .execute(pool)
    .await;

    let body = json!({
        "obj_id": obj_id,
        "category": category,
        "rate": rate,
        "comment": "BDD test rating"
    });
    world.post_json("/rating", &body).await;
    store_rating_id(world);
}

#[when("I get the stored rating")]
async fn get_rating(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("rating_id")
        .expect("No stored rating_id")
        .clone();
    world.get(&format!("/rating/{}", id)).await;
}

#[when(regex = r#"^I update the stored rating with rate (\d+) and comment "([^"]+)"$"#)]
async fn update_rating(world: &mut StepWorld, rate: i32, comment: String) {
    let id = world
        .stored_ids
        .get("rating_id")
        .expect("No stored rating_id")
        .clone();
    let body = json!({
        "rate": rate,
        "comment": comment
    });
    world.put_json(&format!("/rating/{}", id), &body).await;
}

#[when("I delete the stored rating")]
async fn delete_rating(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("rating_id")
        .expect("No stored rating_id")
        .clone();
    world.delete(&format!("/rating/{}", id)).await;
}

#[when("I list ratings")]
async fn list_ratings(world: &mut StepWorld) {
    world.get("/rating").await;
}

// ─── Agreement steps ───

fn store_agreement_id(world: &mut StepWorld) {
    if let Some(json) = &world.response_json {
        if let Some(id) = json.pointer("/item/id").and_then(|v| v.as_i64()) {
            world
                .stored_ids
                .insert("agreement_id".to_string(), id.to_string());
        }
    }
}

#[when(regex = r#"^I create an agreement with name "([^"]+)" and text "([^"]+)"$"#)]
async fn create_agreement(world: &mut StepWorld, name: String, text: String) {
    let body = json!({
        "name": name,
        "text": text
    });
    world.post_json("/admin/agreement", &body).await;
    store_agreement_id(world);
}

#[given(regex = r#"^I have created an agreement with name "([^"]+)"$"#)]
async fn given_created_agreement(world: &mut StepWorld, name: String) {
    let body = json!({
        "name": name,
        "text": "BDD test agreement text for compliance."
    });
    world.post_json("/admin/agreement", &body).await;
    store_agreement_id(world);
}

#[when("I get the stored agreement as admin")]
async fn get_agreement_admin(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("agreement_id")
        .expect("No stored agreement_id")
        .clone();
    world.get(&format!("/admin/agreement/{}", id)).await;
}

#[when("I sign the stored agreement")]
async fn sign_agreement(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("agreement_id")
        .expect("No stored agreement_id")
        .clone();
    let body = json!({
        "agrt_id": id.parse::<i32>().unwrap()
    });
    world.post_json("/agreement", &body).await;
}

#[given("I sign the stored agreement")]
async fn given_sign_agreement(world: &mut StepWorld) {
    sign_agreement(world).await;
}

#[when("I check if the stored agreement is accepted")]
async fn check_agreement_accepted(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("agreement_id")
        .expect("No stored agreement_id")
        .clone();
    world.get(&format!("/agreement/accepted/{}", id)).await;
}

// ─── Chat steps ───

#[when("I upsert chat history with messages")]
async fn upsert_chat(world: &mut StepWorld) {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi there!"}
        ]
    });
    world.put_json("/chat/history", &body).await;
}

#[given("I have upserted chat history")]
async fn given_upserted_chat(world: &mut StepWorld) {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Test message"}
        ]
    });
    world.put_json("/chat/history", &body).await;
    assert_eq!(
        world.status_code.unwrap_or(0),
        200,
        "Chat upsert failed: {}",
        world.response_body.as_deref().unwrap_or("(no body)")
    );
}

#[when("I upsert and then get chat history")]
async fn upsert_then_get_chat(world: &mut StepWorld) {
    let body = json!({
        "messages": [{"role": "user", "content": "Test message"}]
    });
    world.put_json("/chat/history", &body).await;
    assert_eq!(world.status_code.unwrap_or(0), 200, "Chat upsert failed");
    // GET immediately after
    world.get("/chat/history").await;
}

#[when("I get chat history")]
async fn get_chat(world: &mut StepWorld) {
    world.get("/chat/history").await;
}

#[when("I delete chat history")]
async fn delete_chat(world: &mut StepWorld) {
    world.delete("/chat/history").await;
}

// ─── Chat session steps ───

#[when(regex = r#"^I create a chat session titled "(.+)"$"#)]
async fn create_chat_session(world: &mut StepWorld, title: String) {
    let body = json!({
        "title": title,
        "messages": [{"role": "user", "content": "Hello session"}]
    });
    world.post_json("/chat/sessions", &body).await;
    world.store_id_from_response("chat_session_id", "/item/id");
}

#[given(regex = r#"^I have created a chat session titled "(.+)"$"#)]
async fn given_created_chat_session(world: &mut StepWorld, title: String) {
    let body = json!({
        "title": title,
        "messages": [{"role": "user", "content": "Hello session"}]
    });
    world.post_json("/chat/sessions", &body).await;
    let status = world.status_code.unwrap_or(0);
    assert!(
        status == 200 || status == 201,
        "Chat session create failed ({}): {}",
        status,
        world.response_body.as_deref().unwrap_or("(no body)")
    );
    world.store_id_from_response("chat_session_id", "/item/id");
}

#[when("I list my chat sessions")]
async fn list_chat_sessions(world: &mut StepWorld) {
    world.get("/chat/sessions").await;
}

#[when("I get the messages of that session")]
async fn get_messages_of_stored_session(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("chat_session_id")
        .expect("No stored chat_session_id")
        .clone();
    world.get(&format!("/chat/sessions/{}/messages", id)).await;
}

#[when(regex = r#"^I get the messages of session "(.+)"$"#)]
async fn get_messages_of_literal_session(world: &mut StepWorld, id: String) {
    world.get(&format!("/chat/sessions/{}/messages", id)).await;
}

#[when("I delete that session")]
async fn delete_stored_session(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("chat_session_id")
        .expect("No stored chat_session_id")
        .clone();
    world.delete(&format!("/chat/sessions/{}", id)).await;
}

#[when(regex = r#"^I append a message "(.+)" to that session$"#)]
async fn append_message_to_stored_session(world: &mut StepWorld, content: String) {
    let id = world
        .stored_ids
        .get("chat_session_id")
        .expect("No stored chat_session_id")
        .clone();
    let body = json!({ "role": "user", "content": content });
    world
        .post_json(&format!("/chat/sessions/{}/messages", id), &body)
        .await;
}

#[when(regex = r#"^I rename that session to "(.+)"$"#)]
async fn rename_stored_session(world: &mut StepWorld, title: String) {
    let id = world
        .stored_ids
        .get("chat_session_id")
        .expect("No stored chat_session_id")
        .clone();
    let body = json!({ "title": title });
    world.patch(&format!("/chat/sessions/{}", id), &body).await;
}

#[when(regex = r#"^I rename session "(.+)" to "(.+)"$"#)]
async fn rename_literal_session(world: &mut StepWorld, id: String, title: String) {
    let body = json!({ "title": title });
    world.patch(&format!("/chat/sessions/{}", id), &body).await;
}

#[when("I archive that session")]
async fn archive_stored_session(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("chat_session_id")
        .expect("No stored chat_session_id")
        .clone();
    world
        .post_json(&format!("/chat/sessions/{}/archive", id), &json!({}))
        .await;
}

#[when("I unarchive that session")]
async fn unarchive_stored_session(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("chat_session_id")
        .expect("No stored chat_session_id")
        .clone();
    world
        .post_json(&format!("/chat/sessions/{}/unarchive", id), &json!({}))
        .await;
}

#[when("I list my archived chat sessions")]
async fn list_archived_chat_sessions(world: &mut StepWorld) {
    world.get("/chat/sessions?archived=true").await;
}

#[then(regex = r#"^the response body should not contain "(.+)"$"#)]
async fn response_body_should_not_contain(world: &mut StepWorld, needle: String) {
    let body = world.response_body.as_deref().unwrap_or("");
    assert!(
        !body.contains(&needle),
        "Response body unexpectedly contained '{}': {}",
        needle,
        body
    );
}

// ─── DockerHub steps ───

#[when(regex = r#"^I search DockerHub namespaces with query "([^"]+)"$"#)]
async fn search_namespaces(world: &mut StepWorld, query: String) {
    world
        .get(&format!("/dockerhub/namespaces?q={}", query))
        .await;
}

#[when(regex = r#"^I list DockerHub repositories for namespace "([^"]+)"$"#)]
async fn list_repositories(world: &mut StepWorld, namespace: String) {
    world
        .get(&format!("/dockerhub/{}/repositories", namespace))
        .await;
}

#[when(regex = r#"^I list DockerHub tags for namespace "([^"]+)" repository "([^"]+)"$"#)]
async fn list_tags(world: &mut StepWorld, namespace: String, repository: String) {
    world
        .get(&format!(
            "/dockerhub/{}/repositories/{}/tags",
            namespace, repository
        ))
        .await;
}

// ─── Handoff steps ───

#[when("I mint a handoff token for the stored deployment")]
async fn mint_handoff(world: &mut StepWorld) {
    let dep_id = world
        .stored_ids
        .get("deployment_id")
        .expect("No stored deployment_id")
        .clone();
    let body = json!({
        "deployment_id": dep_id.parse::<i32>().unwrap()
    });
    world.post_json("/api/v1/handoff/mint", &body).await;
    // Store the token
    if let Some(json) = &world.response_json {
        if let Some(token) = json
            .pointer("/item/token")
            .or_else(|| json.pointer("/data/token"))
            .and_then(|v| v.as_str())
        {
            world
                .stored_ids
                .insert("handoff_token".to_string(), token.to_string());
        }
    }
}

#[given("I have minted a handoff token for the stored deployment")]
async fn given_minted_handoff(world: &mut StepWorld) {
    mint_handoff(world).await;
}

#[when("I resolve the stored handoff token")]
async fn resolve_stored_handoff(world: &mut StepWorld) {
    let token = world
        .stored_ids
        .get("handoff_token")
        .expect("No stored handoff_token")
        .clone();
    let body = json!({ "token": token });
    world.post_json("/api/v1/handoff/resolve", &body).await;
}

#[given("I resolve the stored handoff token")]
async fn given_resolve_stored_handoff(world: &mut StepWorld) {
    resolve_stored_handoff(world).await;
}

#[when(regex = r#"^I resolve handoff token "([^"]+)"$"#)]
async fn resolve_handoff_token(world: &mut StepWorld, token: String) {
    let body = json!({ "token": token });
    world.post_json("/api/v1/handoff/resolve", &body).await;
}

// ─── Anonymous user step ───

#[given("I switch to anonymous user")]
async fn switch_to_anonymous(world: &mut StepWorld) {
    world.auth_token = "anonymous".to_string();
}

#[then(regex = r#"^I switch to anonymous user$"#)]
async fn then_switch_to_anonymous(world: &mut StepWorld) {
    world.auth_token = "anonymous".to_string();
}

// ─── Admin / User A switching ───

#[given("I switch to admin user")]
async fn given_switch_to_admin(world: &mut StepWorld) {
    world.auth_token = "admin-token".to_string();
}

#[when("I switch to admin user")]
async fn when_switch_to_admin(world: &mut StepWorld) {
    world.auth_token = "admin-token".to_string();
}

#[given("I switch to User A")]
async fn given_switch_to_user_a(world: &mut StepWorld) {
    world.auth_token = "user-a-token".to_string();
}
