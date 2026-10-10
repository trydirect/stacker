use actix_web::web;
use serde_json::json;
use stacker::mcp::{ToolContext, ToolRegistry};
use std::sync::Arc;

/// The Ansible role tools take a role name and read files under the roles
/// directory. A name is a single directory name: an absolute path or a `..`
/// component would let any user read README.md and defaults/main.yml from
/// anywhere on the Stacker host.

const ROLE_TOOLS: &[&str] = &[
    "get_role_details",
    "get_role_requirements",
    "validate_role_vars",
    "deploy_role",
];

fn context() -> ToolContext {
    ToolContext {
        user: Arc::new(stacker::models::User {
            id: "any_user".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            email: "any@example.com".to_string(),
            role: "group_user".to_string(),
            email_confirmed: true,
            mfa_verified: true,
            access_token: None,
        }),
        pg_pool: sqlx::PgPool::connect_lazy("postgres://unused@localhost/unused").unwrap(),
        settings: web::Data::new(stacker::configuration::Settings::default()),
    }
}

#[tokio::test]
async fn role_tools_refuse_paths_outside_the_roles_directory() {
    // A directory that looks like a role, outside /ansible/roles.
    let outside = std::env::temp_dir().join(format!("not-a-role-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(outside.join("defaults")).unwrap();
    std::fs::write(
        outside.join("README.md"),
        "Description\nHOST-FILE-CONTENT\n",
    )
    .unwrap();
    std::fs::write(
        outside.join("defaults/main.yml"),
        "host_secret: HOST-FILE-CONTENT\n",
    )
    .unwrap();

    let names = [
        outside.to_string_lossy().to_string(),
        format!("../../..{}", outside.to_string_lossy()),
    ];
    let registry = ToolRegistry::new();
    let ctx = context();
    let mut failures = Vec::new();
    for tool in ROLE_TOOLS {
        let handler = registry
            .get(tool)
            .unwrap_or_else(|| panic!("{tool} not registered"));
        for name in &names {
            let args = json!({
                "role_name": name,
                "variables": {},
                "server_ip": "127.0.0.1",
            });
            if let Ok(content) = handler.execute(args, &ctx).await {
                failures.push(format!("{tool} accepted role_name {name}: {:?}", content));
            }
        }
    }
    std::fs::remove_dir_all(&outside).ok();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
