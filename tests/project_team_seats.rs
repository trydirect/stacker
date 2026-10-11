//! Team seats: an owner on the Team plan invites people to a project by email.
//!
//! Each test is named after a scenario in
//! `config/shared-fixtures/features/project-team-members.feature`.
//!
//! The harness gives the app its own set of accounts: a mock OAuth `/me` maps
//! a bearer token to a user (id, email, confirmed or not), and a mock User
//! Service answers `/oauth_server/api/me` with that user's plan code. Plan
//! codes sit behind a mutex so a test can move an owner to another plan.

mod common;

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[derive(Clone, Copy)]
struct Account {
    token: &'static str,
    id: &'static str,
    email: &'static str,
    plan: &'static str,
    confirmed: bool,
}

const TEAM: Account = Account {
    token: "tok-team",
    id: "team_owner_id",
    email: "team@try.direct",
    plan: "plan-individual-monthly",
    confirmed: true,
};
const PRO: Account = Account {
    token: "tok-pro",
    id: "pro_user_id",
    email: "pro@try.direct",
    plan: "plan-plus-monthly",
    confirmed: true,
};
const STARTER: Account = Account {
    token: "tok-starter",
    id: "starter_user_id",
    email: "starter@try.direct",
    plan: "plan-basic-monthly",
    confirmed: true,
};
const FREE: Account = Account {
    token: "tok-free",
    id: "free_user_id",
    email: "free@try.direct",
    plan: "plan-free-periodically",
    confirmed: true,
};
const COLLEAGUE: Account = Account {
    token: "tok-colleague",
    id: "colleague_id",
    email: "colleague@try.direct",
    plan: "plan-free-periodically",
    confirmed: true,
};
/// The same person as COLLEAGUE before they confirmed their email.
const COLLEAGUE_UNCONFIRMED: Account = Account {
    token: "tok-colleague-unconfirmed", // pragma: allowlist secret (fake bearer for the mock auth server)
    id: "colleague_id",
    email: "colleague@try.direct",
    plan: "plan-free-periodically",
    confirmed: false,
};
const OTHER: Account = Account {
    token: "tok-other",
    id: "other_id",
    email: "other@try.direct",
    plan: "plan-free-periodically",
    confirmed: true,
};

const ACCOUNTS: &[Account] = &[
    TEAM,
    PRO,
    STARTER,
    FREE,
    COLLEAGUE,
    COLLEAGUE_UNCONFIRMED,
    OTHER,
];

fn account_for(req: &Request) -> Option<Account> {
    let header = req
        .headers
        .iter()
        .find(|(name, _)| name.as_str().eq_ignore_ascii_case("authorization"))
        .map(|(_, values)| values.last().as_str().to_string())?;
    let token = header.strip_prefix("Bearer ")?;
    ACCOUNTS.iter().copied().find(|a| a.token == token)
}

struct AuthResponder;

impl Respond for AuthResponder {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let Some(account) = account_for(req) else {
            return ResponseTemplate::new(401);
        };
        let user = stacker::forms::user::User {
            id: account.id.to_string(),
            email: account.email.to_string(),
            role: "group_user".to_string(),
            email_confirmed: account.confirmed,
            ..Default::default()
        };
        ResponseTemplate::new(200).set_body_json(stacker::forms::user::UserForm { user })
    }
}

struct PlanResponder {
    plans: Arc<Mutex<HashMap<&'static str, String>>>,
}

impl Respond for PlanResponder {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let Some(account) = account_for(req) else {
            return ResponseTemplate::new(401);
        };
        let code = self.plans.lock().unwrap()[account.id].clone();
        ResponseTemplate::new(200).set_body_json(json!({
            "user": { "email": account.email, "plan": { "code": code, "name": code } }
        }))
    }
}

struct App {
    address: String,
    pool: sqlx::PgPool,
    client: reqwest::Client,
    plans: Arc<Mutex<HashMap<&'static str, String>>>,
    user_service: MockServer,
    _auth: MockServer,
}

impl App {
    async fn spawn() -> Option<App> {
        let auth = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .respond_with(AuthResponder)
            .mount(&auth)
            .await;

        let plans = Arc::new(Mutex::new(
            ACCOUNTS
                .iter()
                .map(|a| (a.id, a.plan.to_string()))
                .collect::<HashMap<_, _>>(),
        ));
        let user_service = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/oauth_server/api/me"))
            .respond_with(PlanResponder {
                plans: plans.clone(),
            })
            .mount(&user_service)
            .await;

        let mut configuration =
            stacker::configuration::get_configuration().expect("Failed to get configuration");
        if let Some(ref mut cfg) = configuration.connectors.dockerhub_service {
            cfg.enabled = false;
        }
        configuration.auth_url = format!("{}/me", auth.uri());
        configuration.user_service_url = user_service.uri();
        configuration.database.database_name = uuid::Uuid::new_v4().to_string();

        let pool = match common::configure_database(&configuration.database).await {
            Ok(pool) => pool,
            Err(err) => {
                eprintln!("Skipping tests: failed to connect to postgres: {}", err);
                return None;
            }
        };

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind port");
        let address = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let agent_pool = stacker::helpers::AgentPgPool::new(pool.clone());
        let server = stacker::startup::run(listener, pool.clone(), agent_pool, configuration)
            .await
            .expect("Failed to start app");
        tokio::spawn(server);

        Some(App {
            address,
            pool,
            client: reqwest::Client::builder()
                .pool_max_idle_per_host(0)
                .build()
                .unwrap(),
            plans,
            user_service,
            _auth: auth,
        })
    }

    fn set_plan(&self, account: Account, code: &str) {
        self.plans
            .lock()
            .unwrap()
            .insert(account.id, code.to_string());
    }

    async fn project(&self, owner: Account) -> i32 {
        common::create_test_project(&self.pool, owner.id).await
    }

    async fn call(
        &self,
        as_user: Option<Account>,
        verb: &str,
        route: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let url = format!("{}{}", self.address, route);
        let mut req = match verb {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "DELETE" => self.client.delete(&url),
            other => panic!("unsupported verb {other}"),
        };
        if let Some(account) = as_user {
            req = req.bearer_auth(account.token);
        }
        if let Some(body) = body {
            req = req.json(&body);
        }
        let resp = req.send().await.expect("request failed");
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    async fn invite(&self, owner: Account, project_id: i32, email: &str) -> (u16, Value) {
        self.call(
            Some(owner),
            "POST",
            &format!("/project/{project_id}/invitations"),
            Some(json!({ "email": email, "role": "member" })),
        )
        .await
    }

    /// Invite and return the token from the invite link the owner gets back.
    async fn invite_token(&self, owner: Account, project_id: i32, email: &str) -> String {
        let (status, body) = self.invite(owner, project_id, email).await;
        assert!(
            (200..300).contains(&status),
            "invite {email} failed: {status} {body}"
        );
        let link = body["item"]["invite_link"]
            .as_str()
            .unwrap_or_else(|| panic!("no invite_link in {body}"));
        link.rsplit('/').next().unwrap().to_string()
    }

    async fn accept(&self, account: Account, key: &str) -> (u16, Value) {
        self.call(
            Some(account),
            "POST",
            &format!("/project/invitations/{key}/accept"),
            None,
        )
        .await
    }

    async fn seats(&self, owner: Account, project_id: i32) -> (i64, i64) {
        let (status, body) = self
            .call(
                Some(owner),
                "GET",
                &format!("/project/{project_id}/seats"),
                None,
            )
            .await;
        assert_eq!(status, 200, "seats: {body}");
        (
            body["item"]["seats_used"].as_i64().unwrap(),
            body["item"]["seats_total"].as_i64().unwrap(),
        )
    }

    async fn first_invitation_id(&self, owner: Account, project_id: i32) -> String {
        let (status, list) = self
            .call(
                Some(owner),
                "GET",
                &format!("/project/{project_id}/invitations"),
                None,
            )
            .await;
        assert_eq!(status, 200, "{list}");
        list["list"][0]["id"].as_str().unwrap().to_string()
    }

    async fn member_role(&self, project_id: i32, account: Account) -> Option<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT role FROM project_member WHERE project_id = $1 AND user_id = $2",
        )
        .bind(project_id)
        .bind(account.id)
        .fetch_optional(&self.pool)
        .await
        .unwrap()
    }

    async fn add_member(&self, project_id: i32, user_id: &str, role: &str) {
        sqlx::query(
            "INSERT INTO project_member (project_id, user_id, role, created_by) VALUES ($1, $2, $3, $4)",
        )
        .bind(project_id)
        .bind(user_id)
        .bind(role)
        .bind(TEAM.id)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn invitation_count(&self, project_id: i32) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM project_invitation WHERE project_id = $1",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }
}

fn message(body: &Value) -> String {
    body["message"].as_str().unwrap_or_default().to_string()
}

fn ok(status: u16) -> bool {
    (200..300).contains(&status)
}

// ── Inviting ──

#[tokio::test]
async fn a_team_owner_invites_a_colleague_by_email() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;

    // Mixed case on purpose: the invitation is stored and matched lowercased.
    let (status, body) = app.invite(TEAM, project, "Colleague@Try.Direct").await;
    assert!(ok(status), "{status} {body}");
    assert_eq!(body["item"]["email"], "colleague@try.direct");
    assert_eq!(body["item"]["role"], "member");
    assert_eq!(body["item"]["status"], "pending");
    assert!(body["item"]["invite_link"]
        .as_str()
        .unwrap_or_default()
        .contains("/invitations/"));

    let (status, list) = app
        .call(
            Some(TEAM),
            "GET",
            &format!("/project/{project}/invitations"),
            None,
        )
        .await;
    assert_eq!(status, 200, "{list}");
    let emails: Vec<&str> = list["list"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["email"].as_str())
        .collect();
    assert_eq!(emails, vec!["colleague@try.direct"]);
    // The owner's list never carries the token or its hash.
    assert!(!list.to_string().contains("token"), "{list}");

    assert_eq!(app.seats(TEAM, project).await, (2, 5));
}

#[tokio::test]
async fn owners_below_the_team_plan_cannot_invite() {
    let Some(app) = App::spawn().await else {
        return;
    };
    for owner in [FREE, STARTER, PRO] {
        let project = app.project(owner).await;
        let (status, body) = app.invite(owner, project, COLLEAGUE.email).await;
        assert_eq!(status, 403, "{} got {status} {body}", owner.email);
        assert_eq!(message(&body), "plan_required", "{}", owner.email);
        assert_eq!(app.invitation_count(project).await, 0, "{}", owner.email);
    }
}

#[tokio::test]
async fn the_fifth_seat_cannot_be_filled() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.add_member(project, "m1", "member").await;
    app.add_member(project, "m2", "member").await;
    app.invite_token(TEAM, project, "p1@try.direct").await;
    app.invite_token(TEAM, project, "p2@try.direct").await;
    assert_eq!(app.seats(TEAM, project).await, (5, 5));

    let (status, body) = app.invite(TEAM, project, "one-more@try.direct").await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(message(&body), "seat_limit");
}

#[tokio::test]
async fn an_expired_invitation_frees_its_seat() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.add_member(project, "m1", "member").await;
    app.add_member(project, "m2", "member").await;
    app.invite_token(TEAM, project, "p1@try.direct").await;
    app.invite_token(TEAM, project, "p2@try.direct").await;
    sqlx::query(
        "UPDATE project_invitation SET expires_at = NOW() - INTERVAL '1 minute' WHERE email = 'p2@try.direct'",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    assert_eq!(app.seats(TEAM, project).await, (4, 5));
    let (status, body) = app.invite(TEAM, project, "one-more@try.direct").await;
    assert!(ok(status), "{status} {body}");
    // The expired one can be invited again too.
    let (status, body) = app.invite(TEAM, project, "p2@try.direct").await;
    assert_eq!(status, 409, "seats are full again: {body}");
    assert_eq!(message(&body), "seat_limit");
}

#[tokio::test]
async fn adding_a_member_directly_by_user_id_obeys_the_same_plan_and_seat_rules() {
    let Some(app) = App::spawn().await else {
        return;
    };

    let full = app.project(TEAM).await;
    app.add_member(full, "m1", "member").await;
    app.add_member(full, "m2", "member").await;
    app.invite_token(TEAM, full, "p1@try.direct").await;
    app.invite_token(TEAM, full, "p2@try.direct").await;
    let (status, body) = app
        .call(
            Some(TEAM),
            "POST",
            &format!("/project/{full}/members"),
            Some(json!({ "user_id": OTHER.id, "role": "member" })),
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(message(&body), "seat_limit");

    let pro_project = app.project(PRO).await;
    let (status, body) = app
        .call(
            Some(PRO),
            "POST",
            &format!("/project/{pro_project}/members"),
            Some(json!({ "user_id": OTHER.id, "role": "viewer" })),
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(message(&body), "plan_required");
    assert_eq!(app.member_role(pro_project, OTHER).await, None);

    // A Team owner with free seats can add a member directly, as member or viewer.
    let open = app.project(TEAM).await;
    for (who, role) in [(OTHER, "member"), (COLLEAGUE, "viewer")] {
        let (status, body) = app
            .call(
                Some(TEAM),
                "POST",
                &format!("/project/{open}/members"),
                Some(json!({ "user_id": who.id, "role": role })),
            )
            .await;
        assert!(ok(status), "{role}: {status} {body}");
        assert_eq!(app.member_role(open, who).await.as_deref(), Some(role));
    }
    // The owner is not a member of their own project.
    let (status, _) = app
        .call(
            Some(TEAM),
            "POST",
            &format!("/project/{open}/members"),
            Some(json!({ "user_id": TEAM.id, "role": "member" })),
        )
        .await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn the_owner_cannot_invite_themselves_an_existing_member_or_an_already_invited_email() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.add_member(project, COLLEAGUE.id, "member").await;
    app.invite_token(TEAM, project, OTHER.email).await;

    let (status, body) = app.invite(TEAM, project, "TEAM@try.direct").await;
    assert_eq!(status, 409, "self: {body}");
    let (status, body) = app.invite(TEAM, project, "other@TRY.direct").await;
    assert_eq!(status, 409, "already invited: {body}");
    assert_eq!(message(&body), "already_invited");

    // Members are stored by user id, not email, so an invitation to a
    // member's address can be created; accepting it is refused.
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;
    let (status, body) = app.accept(COLLEAGUE, &token).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(message(&body), "already_member");
    assert_eq!(
        app.member_role(project, COLLEAGUE).await.as_deref(),
        Some("member")
    );

    let (status, _) = app.invite(TEAM, project, "not-an-email").await;
    assert_eq!(status, 400);
    let (status, _) = app
        .call(
            Some(TEAM),
            "POST",
            &format!("/project/{project}/invitations"),
            Some(json!({ "email": "x@try.direct", "role": "owner" })),
        )
        .await;
    assert_eq!(status, 400, "only member and viewer can be invited");
}

#[tokio::test]
async fn only_the_owner_manages_invitations() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.add_member(project, COLLEAGUE.id, "member").await;
    app.invite_token(TEAM, project, "someone@try.direct").await;

    let (status, body) = app.invite(COLLEAGUE, project, "friend@try.direct").await;
    assert_eq!(status, 403, "member invite: {body}");
    for route in ["invitations", "seats", "members"] {
        let (status, _) = app
            .call(
                Some(COLLEAGUE),
                "GET",
                &format!("/project/{project}/{route}"),
                None,
            )
            .await;
        assert_eq!(status, 403, "member GET {route}");
    }

    // Someone with no access gets the same answer as for a missing project.
    let (status, _) = app.invite(PRO, project, "friend@try.direct").await;
    assert_eq!(status, 404);
    let (status, _) = app.invite(PRO, 999_999, "friend@try.direct").await;
    assert_eq!(status, 404);
}

// ── Accepting ──

#[tokio::test]
async fn an_existing_user_accepts_an_invitation_and_becomes_a_member() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;

    let (status, body) = app.accept(COLLEAGUE, &token).await;
    assert!(ok(status), "{status} {body}");
    assert_eq!(body["item"]["project_id"], project);
    assert_eq!(
        app.member_role(project, COLLEAGUE).await.as_deref(),
        Some("member")
    );

    let (_, list) = app
        .call(
            Some(TEAM),
            "GET",
            &format!("/project/{project}/invitations"),
            None,
        )
        .await;
    assert_eq!(list["list"].as_array().map(Vec::len), Some(0), "{list}");

    let (status, shared) = app
        .call(Some(COLLEAGUE), "GET", "/project/shared", None)
        .await;
    assert_eq!(status, 200);
    let shared_here: Vec<&Value> = shared["list"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["id"].as_i64() == Some(project as i64))
        .collect();
    assert_eq!(shared_here.len(), 1, "{shared}");
    assert_eq!(shared_here[0]["role"], "member");
}

#[tokio::test]
async fn someone_without_an_account_signs_up_from_the_invitation_and_comes_back_to_accept_it() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;

    // Anonymous: the invitation page can show what it is without signing in.
    let (status, body) = app
        .call(None, "GET", &format!("/project/invitations/{token}"), None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["item"]["project_name"], "Test Project");
    assert_eq!(body["item"]["invited_by"], TEAM.email);
    assert_eq!(body["item"]["status"], "pending");
    // The invited address is masked: the link alone must not reveal it.
    let shown = body["item"]["email"].as_str().unwrap_or_default();
    assert!(
        shown.contains('*') && !shown.contains("colleague"),
        "{shown}"
    );
    assert!(!body.to_string().contains("token"), "{body}");

    // A made-up token, or an invitation id in place of a token, is a plain 404
    // for anonymous callers.
    let (status, _) = app
        .call(None, "GET", "/project/invitations/not-a-real-token", None)
        .await;
    assert_eq!(status, 404);
    let invitation_id = app.first_invitation_id(TEAM, project).await;
    let (status, _) = app
        .call(
            None,
            "GET",
            &format!("/project/invitations/{invitation_id}"),
            None,
        )
        .await;
    assert_eq!(status, 404);

    // Accepting needs a signed-in account.
    let (status, _) = app
        .call(
            None,
            "POST",
            &format!("/project/invitations/{token}/accept"),
            None,
        )
        .await;
    assert!(status == 401 || status == 403, "anonymous accept: {status}");

    // After signing up and confirming the email, accepting works.
    let (status, body) = app.accept(COLLEAGUE, &token).await;
    assert!(ok(status), "{status} {body}");
    assert_eq!(
        app.member_role(project, COLLEAGUE).await.as_deref(),
        Some("member")
    );
}

#[tokio::test]
async fn a_user_who_lost_the_invitation_link_sees_pending_invitations_after_signing_in() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.invite_token(TEAM, project, "COLLEAGUE@try.direct")
        .await;
    let other_project = app.project(TEAM).await;
    app.invite_token(TEAM, other_project, OTHER.email).await;

    let (status, body) = app
        .call(Some(COLLEAGUE), "GET", "/project/invitations/pending", None)
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["list"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{body}");
    assert_eq!(items[0]["project_id"], project);
    assert_eq!(items[0]["project_name"], "Test Project");
    assert_eq!(items[0]["invited_by"], TEAM.email);

    // The list carries the invitation id, and the invitee can accept by it:
    // the email check is what protects acceptance, not knowing the token.
    let id = items[0]["id"].as_str().unwrap().to_string();
    let (status, body) = app.accept(COLLEAGUE, &id).await;
    assert!(ok(status), "{status} {body}");
    assert_eq!(
        app.member_role(project, COLLEAGUE).await.as_deref(),
        Some("member")
    );

    // Someone else cannot accept OTHER's invitation by its id either.
    let (_, pending) = app
        .call(Some(OTHER), "GET", "/project/invitations/pending", None)
        .await;
    let other_id = pending["list"][0]["id"].as_str().unwrap().to_string();
    let (status, _) = app.accept(COLLEAGUE, &other_id).await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn an_invitation_can_only_be_accepted_by_the_invited_email() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;

    let (status, body) = app.accept(PRO, &token).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(app.member_role(project, PRO).await, None);

    // The invitation is still there for the right person.
    let (status, _) = app.accept(COLLEAGUE, &token).await;
    assert!(ok(status));
}

#[tokio::test]
async fn an_account_with_an_unconfirmed_email_cannot_accept() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;

    let (status, body) = app.accept(COLLEAGUE_UNCONFIRMED, &token).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(message(&body), "email_not_confirmed");
    assert_eq!(app.member_role(project, COLLEAGUE).await, None);
}

#[tokio::test]
async fn expired_revoked_and_used_invitations_cannot_be_accepted() {
    let Some(app) = App::spawn().await else {
        return;
    };

    // older than 7 days
    let project = app.project(TEAM).await;
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;
    let days: f64 = sqlx::query_scalar(
        "SELECT (EXTRACT(EPOCH FROM (expires_at - created_at)) / 86400)::FLOAT8 FROM project_invitation WHERE project_id = $1",
    )
    .bind(project)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(
        (days - 7.0).abs() < 0.01,
        "invitations live 7 days, got {days}"
    );
    sqlx::query("UPDATE project_invitation SET expires_at = NOW() - INTERVAL '1 second' WHERE project_id = $1")
        .bind(project)
        .execute(&app.pool)
        .await
        .unwrap();
    let (status, body) = app.accept(COLLEAGUE, &token).await;
    assert_eq!(status, 410, "expired: {body}");
    assert_eq!(app.member_role(project, COLLEAGUE).await, None);

    // revoked by the owner
    let project = app.project(TEAM).await;
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;
    let invitation_id = app.first_invitation_id(TEAM, project).await;
    let (status, _) = app
        .call(
            Some(TEAM),
            "DELETE",
            &format!("/project/{project}/invitations/{invitation_id}"),
            None,
        )
        .await;
    assert_eq!(status, 204);
    let (status, body) = app.accept(COLLEAGUE, &token).await;
    assert_eq!(status, 410, "revoked: {body}");
    assert_eq!(app.member_role(project, COLLEAGUE).await, None);

    // already accepted
    let project = app.project(TEAM).await;
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;
    let (status, _) = app.accept(COLLEAGUE, &token).await;
    assert!(ok(status));
    let (status, body) = app.accept(COLLEAGUE, &token).await;
    assert_eq!(status, 410, "used: {body}");
}

#[tokio::test]
async fn only_the_owner_can_revoke_and_only_their_own_projects_invitations() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.invite_token(TEAM, project, COLLEAGUE.email).await;
    let invitation_id = app.first_invitation_id(TEAM, project).await;

    // Another owner, using their own project id with this invitation id.
    let other = app.project(PRO).await;
    let (status, _) = app
        .call(
            Some(PRO),
            "DELETE",
            &format!("/project/{other}/invitations/{invitation_id}"),
            None,
        )
        .await;
    assert_eq!(status, 404);
    let (status, _) = app
        .call(
            Some(PRO),
            "DELETE",
            &format!("/project/{project}/invitations/{invitation_id}"),
            None,
        )
        .await;
    assert_eq!(status, 404);
    assert_eq!(app.seats(TEAM, project).await, (2, 5));
}

// ── Removing access and plan changes ──

#[tokio::test]
async fn removing_a_member_or_revoking_an_invitation_takes_effect_at_once() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;
    let (status, _) = app.accept(COLLEAGUE, &token).await;
    assert!(ok(status));
    let other_token = app.invite_token(TEAM, project, OTHER.email).await;
    assert_eq!(app.seats(TEAM, project).await, (3, 5));

    let (status, _) = app
        .call(
            Some(TEAM),
            "DELETE",
            &format!("/project/{project}/members/{}", COLLEAGUE.id),
            None,
        )
        .await;
    assert_eq!(status, 204);
    let invitation_id = app.first_invitation_id(TEAM, project).await;
    let (status, _) = app
        .call(
            Some(TEAM),
            "DELETE",
            &format!("/project/{project}/invitations/{invitation_id}"),
            None,
        )
        .await;
    assert_eq!(status, 204);

    let (status, _) = app
        .call(Some(COLLEAGUE), "GET", &format!("/project/{project}"), None)
        .await;
    assert_eq!(status, 404);
    let (status, _) = app.accept(OTHER, &other_token).await;
    assert_eq!(status, 410);
    assert_eq!(app.seats(TEAM, project).await, (1, 5));
}

#[tokio::test]
async fn when_the_owner_leaves_the_team_plan_members_keep_access_but_no_one_new_can_be_invited() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    let token = app.invite_token(TEAM, project, COLLEAGUE.email).await;
    let pending_token = app.invite_token(TEAM, project, OTHER.email).await;
    let (status, _) = app.accept(COLLEAGUE, &token).await;
    assert!(ok(status));

    app.set_plan(TEAM, "plan-plus-monthly");

    assert_eq!(
        app.member_role(project, COLLEAGUE).await.as_deref(),
        Some("member")
    );
    let (status, body) = app.invite(TEAM, project, "friend@try.direct").await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(message(&body), "plan_required");
    // An invitation sent while on Team still works: the plan is checked when
    // inviting, not when accepting.
    let (status, body) = app.accept(OTHER, &pending_token).await;
    assert!(ok(status), "{status} {body}");
}

#[tokio::test]
async fn a_plan_check_that_cannot_reach_the_user_service_refuses_the_invitation() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.user_service.reset().await;
    Mock::given(method("GET"))
        .and(path("/oauth_server/api/me"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&app.user_service)
        .await;

    let (status, body) = app.invite(TEAM, project, COLLEAGUE.email).await;
    assert!(
        status >= 500,
        "an unknown plan must not be treated as Team: {status} {body}"
    );
    assert_eq!(app.invitation_count(project).await, 0);
}

// ── Working as a member ──

fn deploy_body() -> Value {
    json!({
        "stack": {
            "vars": [],
            "integrated_features": [],
            "extended_features": [],
            "subscriptions": [],
            "form_app": []
        },
        "cloud": { "provider": "aws", "save_token": false },
        "server": { "region": "us-east-1", "server": "t3.small", "os": "ubuntu-24.04", "disk_type": "gp3" }
    })
}

async fn mount_owner_lookup(app: &App) {
    Mock::given(method("GET"))
        .and(path(format!("/api/internal/users/{}", TEAM.id)))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "email": TEAM.email, "email_confirmed": true })),
        )
        .mount(&app.user_service)
        .await;
}

async fn owners_of(app: &App, table: &str, project_id: i32) -> Vec<String> {
    sqlx::query_scalar::<_, String>(&format!(
        "SELECT user_id FROM {table} WHERE project_id = $1"
    ))
    .bind(project_id)
    .fetch_all(&app.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_member_deploys_with_the_owners_saved_cloud_and_the_deployment_belongs_to_the_owner() {
    std::env::set_var("INTERNAL_SERVICES_ACCESS_KEY", "test-internal-key");
    let Some(app) = App::spawn().await else {
        return;
    };
    mount_owner_lookup(&app).await;
    let project = app.project(TEAM).await;
    let cloud = common::create_test_cloud(&app.pool, TEAM.id, "acme-aws", "aws").await;
    app.add_member(project, COLLEAGUE.id, "member").await;

    let (status, body) = app
        .call(
            Some(COLLEAGUE),
            "POST",
            &format!("/project/{project}/deploy/{cloud}"),
            Some(deploy_body()),
        )
        .await;
    let text = body.to_string();
    assert!(
        status != 403 && status != 404 && !text.contains("No cloud configured"),
        "the member must be able to use the owner's saved cloud: {status} {text}"
    );
    // The saved credentials never come back to the member.
    assert!(!text.contains("test-cloud-token"), "{text}");

    let servers = owners_of(&app, "server", project).await;
    assert!(!servers.is_empty(), "no server recorded: {status} {text}");
    assert!(
        servers.iter().all(|owner| owner == TEAM.id),
        "servers must be recorded under the owner: {servers:?}"
    );
    let deployments = owners_of(&app, "deployment", project).await;
    assert!(
        deployments.iter().all(|owner| owner == TEAM.id),
        "deployments must be recorded under the owner: {deployments:?}"
    );

    // The member still cannot use a cloud that is not the owner's.
    let own_cloud = common::create_test_cloud(&app.pool, COLLEAGUE.id, "mine", "aws").await;
    let (status, body) = app
        .call(
            Some(COLLEAGUE),
            "POST",
            &format!("/project/{project}/deploy/{own_cloud}"),
            Some(deploy_body()),
        )
        .await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test]
async fn a_member_deploy_is_refused_when_the_owner_cannot_be_looked_up() {
    std::env::set_var("INTERNAL_SERVICES_ACCESS_KEY", "test-internal-key");
    let Some(app) = App::spawn().await else {
        return;
    };
    // No owner lookup mounted: the User Service answers 404 for it.
    let project = app.project(TEAM).await;
    let cloud = common::create_test_cloud(&app.pool, TEAM.id, "acme-aws", "aws").await;
    app.add_member(project, COLLEAGUE.id, "member").await;

    let (status, body) = app
        .call(
            Some(COLLEAGUE),
            "POST",
            &format!("/project/{project}/deploy/{cloud}"),
            Some(deploy_body()),
        )
        .await;
    assert_eq!(status, 502, "{body}");
    assert_eq!(message(&body), "owner_unavailable");
    assert!(owners_of(&app, "server", project).await.is_empty());
}

#[tokio::test]
async fn a_viewer_cannot_deploy() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    let cloud = common::create_test_cloud(&app.pool, TEAM.id, "acme-aws", "aws").await;
    app.add_member(project, COLLEAGUE.id, "viewer").await;

    let (status, _) = app
        .call(
            Some(COLLEAGUE),
            "POST",
            &format!("/project/{project}/deploy/{cloud}"),
            Some(deploy_body()),
        )
        .await;
    assert_eq!(status, 403);
    assert!(owners_of(&app, "server", project).await.is_empty());
}

// ── Commands on the project's deployments ──

/// The owner's deployment of `project`, with one queued command of theirs.
async fn owner_deployment_with_command(app: &App, project: i32) -> (String, String) {
    let hash = format!("team-{}", uuid::Uuid::new_v4());
    common::create_test_deployment(&app.pool, TEAM.id, project, &hash).await;
    let command_id = format!("cmd_{}", uuid::Uuid::new_v4());
    sqlx::query(
        r#"INSERT INTO commands (id, command_id, deployment_hash, type, status, priority, created_by, created_at, updated_at)
           VALUES (gen_random_uuid(), $1, $2, 'health', 'queued', 'normal', $3, NOW(), NOW())"#,
    )
    .bind(&command_id)
    .bind(&hash)
    .bind(TEAM.id)
    .execute(&app.pool)
    .await
    .expect("Failed to insert command");
    (hash, command_id)
}

fn health_command(hash: &str) -> Value {
    json!({ "deployment_hash": hash, "command_type": "health", "parameters": { "app_code": "all" } })
}

#[tokio::test]
async fn a_member_runs_commands_on_the_projects_deployments() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.add_member(project, COLLEAGUE.id, "member").await;
    let (hash, owner_command) = owner_deployment_with_command(&app, project).await;

    let (status, body) = app
        .call(
            Some(COLLEAGUE),
            "POST",
            "/api/v1/commands",
            Some(health_command(&hash)),
        )
        .await;
    assert!(ok(status), "member queues a command: {status} {body}");
    let queued_by: Vec<String> = sqlx::query_scalar(
        "SELECT created_by FROM commands WHERE deployment_hash = $1 ORDER BY created_at",
    )
    .bind(&hash)
    .fetch_all(&app.pool)
    .await
    .unwrap();
    // The audit trail says who asked for it.
    assert_eq!(
        queued_by,
        vec![TEAM.id.to_string(), COLLEAGUE.id.to_string()]
    );

    let (status, body) = app
        .call(
            Some(COLLEAGUE),
            "GET",
            &format!("/api/v1/commands/{hash}"),
            None,
        )
        .await;
    assert_eq!(status, 200, "member lists commands: {body}");
    let (status, body) = app
        .call(
            Some(COLLEAGUE),
            "GET",
            &format!("/api/v1/commands/{hash}/{owner_command}"),
            None,
        )
        .await;
    assert_eq!(status, 200, "member reads a command: {body}");
    let (status, body) = app
        .call(
            Some(COLLEAGUE),
            "POST",
            &format!("/api/v1/commands/{hash}/{owner_command}/cancel"),
            None,
        )
        .await;
    assert_eq!(status, 200, "member cancels a command: {body}");
}

#[tokio::test]
async fn a_viewer_cannot_run_or_read_commands_and_an_outsider_gets_not_found() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.add_member(project, COLLEAGUE.id, "viewer").await;
    let (hash, owner_command) = owner_deployment_with_command(&app, project).await;

    for (who, expected) in [(COLLEAGUE, 403), (PRO, 404)] {
        let (status, _) = app
            .call(
                Some(who),
                "POST",
                "/api/v1/commands",
                Some(health_command(&hash)),
            )
            .await;
        assert_eq!(status, expected, "{} create", who.email);
        let (status, _) = app
            .call(Some(who), "GET", &format!("/api/v1/commands/{hash}"), None)
            .await;
        assert_eq!(status, expected, "{} list", who.email);
        let (status, _) = app
            .call(
                Some(who),
                "GET",
                &format!("/api/v1/commands/{hash}/{owner_command}"),
                None,
            )
            .await;
        assert_eq!(status, expected, "{} get", who.email);
        let (status, _) = app
            .call(
                Some(who),
                "POST",
                &format!("/api/v1/commands/{hash}/{owner_command}/cancel"),
                None,
            )
            .await;
        assert_eq!(status, expected, "{} cancel", who.email);
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM commands WHERE deployment_hash = $1")
        .bind(&hash)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "nothing was queued");
    let status: String = sqlx::query_scalar("SELECT status FROM commands WHERE command_id = $1")
        .bind(&owner_command)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(status, "queued", "nothing was cancelled");
}

#[tokio::test]
async fn a_member_can_point_deploy_app_at_the_shared_project() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    app.add_member(project, COLLEAGUE.id, "member").await;
    let (hash, _) = owner_deployment_with_command(&app, project).await;
    let other = app.project(PRO).await;

    // Into the shared project: allowed past the project check.
    let (status, body) = app
        .call(
            Some(COLLEAGUE),
            "POST",
            "/api/v1/commands",
            Some(json!({
                "deployment_hash": hash,
                "command_type": "deploy_app",
                "parameters": { "app_code": "web", "deployment_id": project }
            })),
        )
        .await;
    assert!(
        status != 403 && status != 404,
        "the shared project must be accepted: {status} {body}"
    );

    // Into someone else's project: refused.
    let (status, body) = app
        .call(
            Some(COLLEAGUE),
            "POST",
            "/api/v1/commands",
            Some(json!({
                "deployment_hash": hash,
                "command_type": "deploy_app",
                "parameters": { "app_code": "web", "deployment_id": other }
            })),
        )
        .await;
    assert_eq!(status, 404, "{body}");
}

// ── Member list for the owner ──

#[tokio::test]
async fn the_owner_sees_who_the_members_are() {
    let Some(app) = App::spawn().await else {
        return;
    };
    let project = app.project(TEAM).await;
    let token = app
        .invite_token(TEAM, project, "Colleague@Try.Direct")
        .await;
    let (status, _) = app.accept(COLLEAGUE, &token).await;
    assert!(ok(status));
    // Added by id, before invitations existed: no address known.
    app.add_member(project, "legacy_viewer_id", "viewer").await;

    let (status, body) = app
        .call(
            Some(TEAM),
            "GET",
            &format!("/project/{project}/members"),
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let members = body["list"].as_array().unwrap();
    let email_of = |id: &str| {
        members
            .iter()
            .find(|m| m["user_id"] == id)
            .unwrap_or_else(|| panic!("{id} not listed: {body}"))["email"]
            .clone()
    };
    assert_eq!(email_of(COLLEAGUE.id), json!("colleague@try.direct"));
    assert_eq!(email_of("legacy_viewer_id"), Value::Null);
}
