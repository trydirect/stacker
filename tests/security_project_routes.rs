mod common;

use std::collections::BTreeSet;

/// IDOR sweep over every project route.
///
/// The per-route tests in security_project.rs covered list/get/update/delete
/// only; deploy went unchecked for years because nothing forced a test for it.
/// This file does two things:
///
/// 1. `every_project_route_is_in_the_sweep` reads the routes registered in
///    `project_scope` (src/startup.rs) and fails when a route that takes a
///    project id is missing from `ROUTES` below. A new route cannot ship
///    without being swept.
/// 2. `other_user_gets_the_same_answer_as_for_a_missing_project` calls every
///    route as User B against User A's project, and against a project id that
///    does not exist. Both answers must be 404 (or 403) and identical: a
///    handler that does not check ownership goes further on a real project
///    and answers differently (200, 400, 500, another message).
///
/// Bodies are valid requests, so a handler cannot hide behind input
/// validation: any difference comes from the project lookup.

const MISSING_PROJECT_ID: i32 = 2_147_480_000;
const APP_CODE: &str = "web";
const DEPLOYMENT_HASH: &str = "sweep-deployment-hash";

/// (method, path relative to the project scope). `{id}` is the project,
/// `{cloud_id}` a saved cloud of the project owner.
const ROUTES: &[(&str, &str)] = &[
    ("GET", "/{id}"),
    ("GET", "/{id}/compose"),
    ("GET", "/{id}/members"),
    ("POST", "/{id}/members"),
    ("DELETE", "/{id}/members/{member_user_id}"),
    ("GET", "/{id}/apps"),
    ("POST", "/{id}/apps"),
    ("GET", "/{id}/apps/{code}"),
    ("GET", "/{id}/apps/{code}/config"),
    ("GET", "/{id}/apps/{code}/env"),
    ("PUT", "/{id}/apps/{code}/env"),
    ("DELETE", "/{id}/apps/{code}/env/{name}"),
    ("PUT", "/{id}/apps/{code}/ports"),
    ("PUT", "/{id}/apps/{code}/domain"),
    ("GET", "/{id}/apps/{code}/secrets"),
    ("GET", "/{id}/apps/{code}/secrets/{name}"),
    ("PUT", "/{id}/apps/{code}/secrets/{name}"),
    ("DELETE", "/{id}/apps/{code}/secrets/{name}"),
    ("GET", "/{id}/containers/discover"),
    ("POST", "/{id}/containers/import"),
    ("POST", "/{id}/deploy"),
    ("POST", "/{id}/deploy/{cloud_id}"),
    ("POST", "/{id}/rollback"),
    ("PUT", "/{id}"),
    ("PUT", "/{id}/sync"),
    ("PATCH", "/{id}/protection"),
    ("GET", "/{id}/invitations"),
    ("POST", "/{id}/invitations"),
    ("GET", "/{id}/seats"),
    ("GET", "/{id}/clouds"),
    // Destructive ones last, so a missing check cannot hide the others.
    ("DELETE", "/{id}/apps/{code}"),
    ("DELETE", "/{id}/invitations/{invitation_id}"),
    ("DELETE", "/{id}"),
];

/// A pending invitation on User A's project, so revoking has a real target.
const SWEEP_INVITATION_ID: &str = "5eed0000-0000-4000-8000-000000000001";

fn body_for(method: &str, path: &str) -> Option<serde_json::Value> {
    use serde_json::json;
    let body = match (method, path) {
        ("POST", "/{id}/members") => json!({ "user_id": common::USER_B_ID, "role": "viewer" }),
        ("POST", "/{id}/invitations") => {
            json!({ "email": "intruder@example.com", "role": "member" })
        }
        ("POST", "/{id}/apps") => json!({ "code": "intruder", "image": "nginx:alpine" }),
        ("PUT", "/{id}/apps/{code}/env") => json!({ "variables": { "INJECTED": "1" } }),
        ("PUT", "/{id}/apps/{code}/ports") => {
            json!({ "ports": [{ "host": 8081, "container": 80 }] })
        }
        ("PUT", "/{id}/apps/{code}/domain") => {
            json!({ "domain": "intruder.example.com", "ssl_enabled": false })
        }
        ("PUT", "/{id}/apps/{code}/secrets/{name}") => json!({ "value": "injected" }),
        ("POST", "/{id}/containers/import") => json!({
            "containers": [{
                "container_name": "intruder",
                "app_code": "intruder",
                "name": "intruder",
                "image": "nginx:alpine"
            }]
        }),
        ("POST", "/{id}/deploy") | ("POST", "/{id}/deploy/{cloud_id}") => json!({
            "stack": {
                "vars": [],
                "integrated_features": [],
                "extended_features": [],
                "subscriptions": [],
                "form_app": []
            },
            "cloud": { "provider": "aws", "cloud_token": "test-cloud-token", "save_token": false },
            "server": { "region": "us-east-1", "server": "t3.small", "os": "ubuntu-24.04", "disk_type": "gp3" }
        }),
        ("POST", "/{id}/rollback") => json!({ "version": "1.0.0" }),
        ("PUT", "/{id}") | ("PUT", "/{id}/sync") => {
            json!({ "custom": { "custom_stack_code": "intruder" } })
        }
        ("PATCH", "/{id}/protection") => json!({ "is_protected": false }),
        _ => return None,
    };
    Some(body)
}

fn fill(path: &str, project_id: i32, cloud_id: i32) -> String {
    let mut url = path
        .replace("{id}", &project_id.to_string())
        .replace("{cloud_id}", &cloud_id.to_string())
        .replace("{code}", APP_CODE)
        .replace("{member_user_id}", common::USER_B_ID)
        .replace("{invitation_id}", SWEEP_INVITATION_ID)
        .replace("{name}", "SOME_NAME");
    if path.ends_with("/containers/discover") || path == "/{id}/apps/{code}" {
        url.push_str(&format!("?deployment_hash={}", DEPLOYMENT_HASH));
    }
    url
}

async fn call(
    client: &reqwest::Client,
    base: &str,
    method: &str,
    path: &str,
    project_id: i32,
    cloud_id: i32,
) -> (u16, String) {
    let url = format!("{}{}", base, fill(path, project_id, cloud_id));
    let mut req = client
        .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), url)
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN));
    if let Some(body) = body_for(method, path) {
        req = req.json(&body);
    }
    let resp = req.send().await.expect("request failed");
    let status = resp.status().as_u16();
    (status, resp.text().await.unwrap_or_default())
}

#[tokio::test]
async fn other_user_gets_the_same_answer_as_for_a_missing_project() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let pool = &app.db_pool;

    // User A's project, with an app, a deployment and a saved cloud, so that
    // every route has something real behind it.
    let project_id = {
        use sqlx::Row;
        sqlx::query(
            r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
               VALUES (gen_random_uuid(), $1, 'Sweep Project', '{}'::jsonb, $2, NOW(), NOW())
               RETURNING id"#,
        )
        .bind(common::USER_A_ID)
        .bind(serde_json::json!({ "custom": { "locked_cloud_provider": "htz" } }))
        .fetch_one(pool)
        .await
        .expect("Failed to insert project")
        .get::<i32, _>("id")
    };
    common::create_test_deployment(pool, common::USER_A_ID, project_id, DEPLOYMENT_HASH).await;
    let cloud_id = common::create_test_cloud(pool, common::USER_A_ID, "a-sweep", "aws").await;
    sqlx::query(
        r#"INSERT INTO project_invitation
               (id, project_id, email, role, token_hash, invited_by, invited_by_email, expires_at)
           VALUES ($1::uuid, $2, 'invitee@example.com', 'member', 'sweep-token-hash', $3,
                   'a@example.com', NOW() + INTERVAL '7 days')"#,
    )
    .bind(SWEEP_INVITATION_ID)
    .bind(project_id)
    .bind(common::USER_A_ID)
    .execute(pool)
    .await
    .expect("Failed to insert invitation");

    let client = reqwest::Client::new();
    let created = client
        .post(format!("{}/project/{}/apps", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&serde_json::json!({ "code": APP_CODE, "image": "nginx:alpine" }))
        .send()
        .await
        .expect("request failed");
    assert!(
        created.status().is_success(),
        "owner could not create the fixture app: {}",
        created.status()
    );

    let mut failures = Vec::new();
    for prefix in ["/project", "/api/v1/project"] {
        let base = format!("{}{}", &app.address, prefix);
        for (method, path) in ROUTES {
            let (missing_status, missing_body) =
                call(&client, &base, method, path, MISSING_PROJECT_ID, cloud_id).await;
            let (foreign_status, foreign_body) =
                call(&client, &base, method, path, project_id, cloud_id).await;

            let route = format!("{} {}{}", method, prefix, path);
            if foreign_status != 404 && foreign_status != 403 {
                failures.push(format!(
                    "IDOR  {route}: other user got {foreign_status} {foreign_body}"
                ));
            } else if (foreign_status, &foreign_body) != (missing_status, &missing_body) {
                failures.push(format!(
                    "LEAK  {route}: other user got {foreign_status} {foreign_body}, \
                     missing project got {missing_status} {missing_body}"
                ));
            }
        }
    }

    // Nothing User B sent may have changed User A's data.
    let still_there: Option<(String,)> =
        sqlx::query_as("SELECT user_id FROM project WHERE id = $1")
            .bind(project_id)
            .fetch_optional(pool)
            .await
            .unwrap();
    if still_there.as_ref().map(|r| r.0.as_str()) != Some(common::USER_A_ID) {
        failures.push("DATA  User A's project was deleted or reassigned".to_string());
    }
    let apps: Vec<(String,)> =
        sqlx::query_as("SELECT code FROM project_app WHERE project_id = $1 ORDER BY code")
            .bind(project_id)
            .fetch_all(pool)
            .await
            .unwrap();
    if apps.iter().map(|r| r.0.as_str()).collect::<Vec<_>>() != vec![APP_CODE] {
        failures.push(format!("DATA  User A's apps changed: {:?}", apps));
    }
    let members: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM project_member WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .unwrap();
    if members.0 != 0 {
        failures.push("DATA  User B added a member to User A's project".to_string());
    }
    let invitations: Vec<(String, String)> = sqlx::query_as(
        "SELECT email, status FROM project_invitation WHERE project_id = $1 ORDER BY email",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .unwrap();
    if invitations != vec![("invitee@example.com".to_string(), "pending".to_string())] {
        failures.push(format!(
            "DATA  User B changed User A's invitations: {:?}",
            invitations
        ));
    }

    assert!(
        failures.is_empty(),
        "{} project route(s) let another user through:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Every route in `project_scope` that takes a project id, as (METHOD, path).
fn registered_project_routes() -> BTreeSet<(String, String)> {
    let root = env!("CARGO_MANIFEST_DIR");
    let startup = std::fs::read_to_string(format!("{root}/src/startup.rs")).unwrap();
    let scope = startup
        .split("fn project_scope")
        .nth(1)
        .expect("project_scope not found in src/startup.rs");
    let scope = &scope[..scope.find("\n}").expect("end of project_scope")];

    let mut routes = BTreeSet::new();
    for line in scope.lines() {
        let Some(rest) = line
            .trim()
            .strip_prefix(".service(crate::routes::project::")
        else {
            continue;
        };
        let target = rest.trim_end_matches(')');
        let (module, handler) = target.split_once("::").expect("module::handler");
        let source =
            std::fs::read_to_string(format!("{root}/src/routes/project/{module}.rs")).unwrap();
        let lines: Vec<&str> = source.lines().collect();
        let fn_line = lines
            .iter()
            .position(|l| {
                l.trim_start()
                    .starts_with(&format!("pub async fn {handler}("))
            })
            .unwrap_or_else(|| panic!("handler {module}::{handler} not found"));
        let attr = lines[..fn_line]
            .iter()
            .rev()
            .map(|l| l.trim())
            .find(|l| {
                ["#[get(", "#[post(", "#[put(", "#[patch(", "#[delete("]
                    .iter()
                    .any(|p| l.starts_with(p))
            })
            .unwrap_or_else(|| panic!("route attribute for {module}::{handler} not found"));
        let method = attr[2..attr.find('(').unwrap()].to_uppercase();
        let path = attr
            .split('"')
            .nth(1)
            .unwrap()
            .replace("{project_id}", "{id}");
        if path.starts_with("/{id}") {
            routes.insert((method, path));
        }
    }
    routes
}

#[test]
fn every_project_route_is_in_the_sweep() {
    let swept: BTreeSet<(String, String)> = ROUTES
        .iter()
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .collect();
    let registered = registered_project_routes();
    assert!(
        registered.len() > 20,
        "parsed only {} routes - the parser no longer understands src/startup.rs",
        registered.len()
    );
    let unswept: Vec<_> = registered.difference(&swept).collect();
    assert!(
        unswept.is_empty(),
        "project routes not covered by the IDOR sweep - add them to ROUTES: {:?}",
        unswept
    );
    let stale: Vec<_> = swept.difference(&registered).collect();
    assert!(
        stale.is_empty(),
        "ROUTES lists routes that no longer exist: {:?}",
        stale
    );
}

// ── Team roles ──
//
// A project member works on the stack like the owner; a viewer only sees the
// project in their shared list and its deployments. Owner-only: secrets,
// deletion, protection, members, invitations and seats.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Need {
    /// Read the stack (payload, compose, apps, env): owner and member.
    View,
    /// Change or deploy the stack: owner and member.
    Manage,
    /// Owner only.
    Owner,
}

/// The role each project route needs. Every entry of ROUTES must be here.
const NEEDS: &[(&str, &str, Need)] = &[
    ("GET", "/{id}", Need::View),
    ("GET", "/{id}/compose", Need::View),
    ("GET", "/{id}/members", Need::Owner),
    ("POST", "/{id}/members", Need::Owner),
    ("DELETE", "/{id}/members/{member_user_id}", Need::Owner),
    ("GET", "/{id}/apps", Need::View),
    ("POST", "/{id}/apps", Need::Manage),
    ("GET", "/{id}/apps/{code}", Need::View),
    ("GET", "/{id}/apps/{code}/config", Need::View),
    ("GET", "/{id}/apps/{code}/env", Need::View),
    ("PUT", "/{id}/apps/{code}/env", Need::Manage),
    ("DELETE", "/{id}/apps/{code}/env/{name}", Need::Manage),
    ("PUT", "/{id}/apps/{code}/ports", Need::Manage),
    ("PUT", "/{id}/apps/{code}/domain", Need::Manage),
    ("GET", "/{id}/apps/{code}/secrets", Need::Owner),
    ("GET", "/{id}/apps/{code}/secrets/{name}", Need::Owner),
    ("PUT", "/{id}/apps/{code}/secrets/{name}", Need::Owner),
    ("DELETE", "/{id}/apps/{code}/secrets/{name}", Need::Owner),
    ("GET", "/{id}/containers/discover", Need::View),
    ("POST", "/{id}/containers/import", Need::Manage),
    ("POST", "/{id}/deploy", Need::Manage),
    ("POST", "/{id}/deploy/{cloud_id}", Need::Manage),
    ("POST", "/{id}/rollback", Need::Manage),
    ("PUT", "/{id}", Need::Manage),
    ("PUT", "/{id}/sync", Need::Manage),
    ("PATCH", "/{id}/protection", Need::Owner),
    ("GET", "/{id}/invitations", Need::Owner),
    ("POST", "/{id}/invitations", Need::Owner),
    ("GET", "/{id}/seats", Need::Owner),
    ("GET", "/{id}/clouds", Need::Manage),
    ("DELETE", "/{id}/apps/{code}", Need::Manage),
    ("DELETE", "/{id}/invitations/{invitation_id}", Need::Owner),
    ("DELETE", "/{id}", Need::Owner),
];

#[test]
fn every_project_route_has_a_role() {
    let classified: BTreeSet<(&str, &str)> = NEEDS.iter().map(|(m, p, _)| (*m, *p)).collect();
    let missing: Vec<_> = ROUTES
        .iter()
        .filter(|(m, p)| !classified.contains(&(*m, *p)))
        .collect();
    assert!(
        missing.is_empty(),
        "project routes with no role in NEEDS: {missing:?}"
    );
}

/// User A's project with an app, a deployment (by `hash`) and a pending
/// invitation, like the IDOR sweep's fixture.
async fn role_fixture(app: &common::TwoUserTestApp, hash: &str) -> i32 {
    use sqlx::Row;
    let pool = &app.db_pool;
    let project_id = sqlx::query(
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
           VALUES (gen_random_uuid(), $1, 'Role Project', '{}'::jsonb, $2, NOW(), NOW())
           RETURNING id"#,
    )
    .bind(common::USER_A_ID)
    .bind(serde_json::json!({ "custom": { "locked_cloud_provider": "htz" } }))
    .fetch_one(pool)
    .await
    .expect("Failed to insert project")
    .get::<i32, _>("id");
    common::create_test_deployment(pool, common::USER_A_ID, project_id, hash).await;
    sqlx::query(
        r#"INSERT INTO project_invitation
               (id, project_id, email, role, token_hash, invited_by, invited_by_email, expires_at)
           VALUES (gen_random_uuid(), $1, 'invitee@example.com', 'member', $2, $3,
                   'a@example.com', NOW() + INTERVAL '7 days')"#,
    )
    .bind(project_id)
    .bind(format!("role-token-{hash}"))
    .bind(common::USER_A_ID)
    .execute(pool)
    .await
    .unwrap();
    let created = reqwest::Client::new()
        .post(format!("{}/project/{}/apps", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&serde_json::json!({ "code": APP_CODE, "image": "nginx:alpine" }))
        .send()
        .await
        .expect("request failed");
    assert!(
        created.status().is_success(),
        "fixture app: {}",
        created.status()
    );
    project_id
}

#[allow(clippy::too_many_arguments)]
async fn call_as(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    method: &str,
    path: &str,
    project_id: i32,
    cloud_id: i32,
    hash: &str,
) -> (u16, String) {
    let url = format!(
        "{}{}",
        base,
        fill(path, project_id, cloud_id).replace(DEPLOYMENT_HASH, hash)
    );
    let mut req = client
        .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), url)
        .header("Authorization", format!("Bearer {token}"));
    if let Some(body) = body_for(method, path) {
        req = req.json(&body);
    }
    let resp = req.send().await.expect("request failed");
    let status = resp.status().as_u16();
    (status, resp.text().await.unwrap_or_default())
}

#[tokio::test]
async fn members_and_viewers_get_what_their_role_allows() {
    let Some(app) = common::spawn_app_two_users_on_team_plan().await else {
        return;
    };
    let pool = &app.db_pool;
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap();
    let base = format!("{}/project", &app.address);
    let cloud_id = common::create_test_cloud(pool, common::USER_A_ID, "a-roles", "aws").await;
    let mut failures = Vec::new();

    // A viewer may not read or change anything in the stack: 403 everywhere.
    let viewed = role_fixture(&app, "role-viewer").await;
    sqlx::query("INSERT INTO project_member (project_id, user_id, role, created_by) VALUES ($1, $2, 'viewer', $3)")
        .bind(viewed)
        .bind(common::USER_B_ID)
        .bind(common::USER_A_ID)
        .execute(pool)
        .await
        .unwrap();
    for (method, path) in ROUTES {
        let (status, body) = call_as(
            &client,
            &base,
            common::USER_B_TOKEN,
            method,
            path,
            viewed,
            cloud_id,
            "role-viewer",
        )
        .await;
        if status != 403 {
            failures.push(format!(
                "VIEWER {method} {path}: expected 403, got {status} {body}"
            ));
        }
    }

    // A member is treated exactly like the owner on stack routes: same status
    // as the owner gets on an identical project. Owner-only routes are 403.
    let mine = role_fixture(&app, "role-owner").await;
    let shared = role_fixture(&app, "role-member").await;
    sqlx::query("INSERT INTO project_member (project_id, user_id, role, created_by) VALUES ($1, $2, 'member', $3)")
        .bind(shared)
        .bind(common::USER_B_ID)
        .bind(common::USER_A_ID)
        .execute(pool)
        .await
        .unwrap();
    for (method, path, need) in NEEDS {
        let (member_status, member_body) = call_as(
            &client,
            &base,
            common::USER_B_TOKEN,
            method,
            path,
            shared,
            cloud_id,
            "role-member",
        )
        .await;
        if *need == Need::Owner {
            if member_status != 403 {
                failures.push(format!(
                    "MEMBER {method} {path}: owner-only, expected 403, got {member_status} {member_body}"
                ));
            }
            continue;
        }
        let (owner_status, owner_body) = call_as(
            &client,
            &base,
            common::USER_A_TOKEN,
            method,
            path,
            mine,
            cloud_id,
            "role-owner",
        )
        .await;
        if member_status != owner_status {
            failures.push(format!(
                "MEMBER {method} {path}: member got {member_status} {member_body}, \
                 owner got {owner_status} {owner_body}"
            ));
        }
    }

    // The owner-only refusals left the member's project intact.
    let still: Option<(String,)> = sqlx::query_as("SELECT user_id FROM project WHERE id = $1")
        .bind(shared)
        .fetch_optional(pool)
        .await
        .unwrap();
    if still.map(|r| r.0) != Some(common::USER_A_ID.to_string()) {
        failures.push("DATA  the member deleted or took over the project".to_string());
    }

    assert!(
        failures.is_empty(),
        "{} role check(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
