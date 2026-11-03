mod common;

use actix_web::web;
use serde_json::json;
use stacker::mcp::{ToolContext, ToolRegistry};
use std::sync::Arc;

// MCP tools that cannot do what they claim must return an error, not a
// success-shaped answer.
//
// An AI client has only the tool's answer to go on. {"status":"queued"} or
// {"status":"pending"} from a tool that queued nothing is indistinguishable
// from real work in progress, so the client reports the job as done and moves
// on. For a firewall that means reporting a server as locked down while it is
// untouched, which is the dangerous direction.
//
// This needs the database because the firewall tool resolves the deployment
// before it reaches the execution-method branch.

const DEPLOYMENT_HASH: &str = "mcpunimplementeddeploymenthash0001";

fn context(pool: &sqlx::PgPool) -> ToolContext {
    ToolContext {
        user: Arc::new(stacker::models::User {
            id: common::USER_A_ID.to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            email: common::USER_A_EMAIL.to_string(),
            role: "group_user".to_string(),
            email_confirmed: true,
            mfa_verified: true,
            access_token: None,
        }),
        pg_pool: pool.clone(),
        settings: web::Data::new(stacker::configuration::Settings::default()),
    }
}

#[tokio::test]
async fn firewall_ssh_method_refuses_instead_of_reporting_a_fake_pending() {
    let app = match common::spawn_app_two_users().await {
        Some(app) => app,
        None => return,
    };

    // A real deployment owned by the caller, so the tool gets past resolution
    // and actually reaches the execution-method branch.
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    common::create_test_deployment(&app.db_pool, common::USER_A_ID, project_id, DEPLOYMENT_HASH)
        .await;

    let registry = ToolRegistry::new();
    let handler = registry
        .get("configure_firewall")
        .expect("configure_firewall not registered");

    let result = handler
        .execute(
            json!({
                "deployment_hash": DEPLOYMENT_HASH,
                "execution_method": "ssh",
                "action": "apply",
                "public_ports": [{"port": 443, "protocol": "tcp"}],
            }),
            &context(&app.db_pool),
        )
        .await;

    let error = match result {
        Err(error) => error,
        Ok(content) => panic!("the ssh execution method answered success: {content:?}"),
    };
    assert!(
        error.to_lowercase().contains("not implemented"),
        "the refusal must say it is not implemented, got: {error}"
    );
    assert!(
        !error.contains("Deployment not found"),
        "the test did not reach the execution-method branch: {error}"
    );

    // The tool list must not offer 'ssh' as a working method either, since that
    // is what an AI client picks a method from.
    let description = handler.schema().description.to_lowercase();
    assert!(
        description.contains("not implemented"),
        "configure_firewall still advertises ssh as usable: {description}"
    );
}

/// The status_panel method must keep working, so this suite cannot go green by
/// the tool refusing everything.
///
/// It also pins the spelling: the tool's JSON schema advertises `status_panel`,
/// which serde rejected until 2026-10-10 because the enum used
/// `rename_all = "lowercase"` and so only accepted `statuspanel`. Both
/// spellings are accepted now.
#[tokio::test]
async fn firewall_status_panel_method_still_works() {
    let app = match common::spawn_app_two_users().await {
        Some(app) => app,
        None => return,
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    common::create_test_deployment(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        "mcpunimplementeddeploymenthash0002",
    )
    .await;

    let registry = ToolRegistry::new();
    let handler = registry
        .get("configure_firewall")
        .expect("configure_firewall not registered");

    let result = handler
        .execute(
            json!({
                "deployment_hash": "mcpunimplementeddeploymenthash0002",
                "execution_method": "status_panel",
                "action": "apply",
                "public_ports": [{"port": 443, "protocol": "tcp"}],
            }),
            &context(&app.db_pool),
        )
        .await;

    assert!(
        result.is_ok(),
        "status_panel must still queue a firewall command: {:?}",
        result.err()
    );

    // The spelling the schema used to reject, and the one it always accepted.
    for spelling in ["status_panel", "statuspanel"] {
        let accepted = handler
            .execute(
                json!({
                    "deployment_hash": "mcpunimplementeddeploymenthash0002",
                    "execution_method": spelling,
                    "action": "apply",
                    "public_ports": [{"port": 443, "protocol": "tcp"}],
                }),
                &context(&app.db_pool),
            )
            .await;
        assert!(
            accepted.is_ok(),
            "execution_method {spelling:?} must be accepted: {:?}",
            accepted.err()
        );
    }
}
