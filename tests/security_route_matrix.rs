mod common;

use std::collections::{BTreeMap, BTreeSet};

/// Authorization matrix over every HTTP route the server registers.
///
/// `every_route_is_classified` reads src/startup.rs, resolves each
/// `.service(...)` to its handler's route attribute and fails when a route
/// is missing from the tables below, or when a table names a route that no
/// longer exists. A new route cannot ship without a decision about who may
/// call it.
///
/// The other tests check the decisions against the running server:
/// - `regular_user_is_refused_on_admin_routes`: User B (group_user) gets 403
///   on every ADMIN route.
/// - `agent_routes_refuse_a_user_session`: a user's OAuth token is not an
///   agent token.
/// - `other_user_gets_the_same_answer_as_for_a_missing_resource`: every
///   SWEEP route is called as User B against User A's resources and against
///   resources that do not exist; both answers must be 404 (or 403) and
///   identical, and User A's data must be unchanged afterwards.
///
/// Project routes (`/project/{id}/...`) are swept in security_project_routes.rs.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Only group_admin may call it (checked live).
    Admin,
    /// Authenticated by an agent token, not a user session (checked live).
    Agent,
    /// No owner: public catalog data, health, metrics.
    Public,
    /// Takes no reference to another user's resource: lists, creates or
    /// changes the caller's own records.
    Own,
    /// Authenticated by something other than a user session: an internal
    /// service key, a webhook signature, a purchase or handoff token.
    Token,
    /// Swept for IDOR by the test below.
    Sweep,
    /// Swept for IDOR in security_project_routes.rs.
    ProjectSweep,
}
use Kind::*;

/// (method, full path as registered, kind, note). Paths keep the parameter
/// names the handlers declare.
const ROUTES: &[(&str, &str, Kind, &str)] = &[
    // health, metrics, catalogs
    ("GET", "/health_check", Public, ""),
    ("GET", "/health_check/metrics", Public, ""),
    ("GET", "/metrics", Public, ""),
    ("GET", "/api/categories", Public, ""),
    ("GET", "/api/templates", Public, "approved templates"),
    (
        "GET",
        "/api/templates/{slug}",
        Public,
        "approved template detail",
    ),
    (
        "GET",
        "/api/v1/templates/{slug}",
        Public,
        "approved template detail",
    ),
    ("GET", "/api/templates/{id}/rating/summary", Public, ""),
    (
        "GET",
        "/api/templates/{id}/increment-view-count",
        Public,
        "counter",
    ),
    (
        "GET",
        "/api/templates/{id}/increment-deploy-count",
        Public,
        "counter",
    ),
    (
        "GET",
        "/api/vendors/{vendor}",
        Public,
        "public vendor profile",
    ),
    ("GET", "/api/v1/marketplace/applications", Public, ""),
    ("GET", "/rating", Public, "visible ratings"),
    ("GET", "/rating/{id}", Public, "visible rating"),
    ("GET", "/agreement/{id}", Public, "agreement text"),
    ("GET", "/api/agreement/{id}", Public, "agreement text"),
    (
        "GET",
        "/dockerhub/namespaces",
        Public,
        "Docker Hub search proxy",
    ),
    (
        "GET",
        "/dockerhub/{namespace}/repositories",
        Public,
        "Docker Hub search proxy",
    ),
    (
        "GET",
        "/dockerhub/{namespace}/repositories/{repository}/tags",
        Public,
        "Docker Hub search proxy",
    ),
    ("POST", "/dockerhub/events", Own, "logs a search event"),
    // the caller's own records
    ("GET", "/project", Own, ""),
    ("POST", "/project", Own, ""),
    ("GET", "/project/shared", Own, ""),
    ("GET", "/api/v1/project", Own, ""),
    ("POST", "/api/v1/project", Own, ""),
    ("GET", "/api/v1/project/shared", Own, ""),
    ("GET", "/cloud", Own, ""),
    ("POST", "/cloud", Own, ""),
    ("GET", "/server", Own, ""),
    (
        "POST",
        "/server/ssh-key/validate-all",
        Own,
        "the caller's servers",
    ),
    ("POST", "/client", Own, ""),
    ("POST", "/rating", Own, ""),
    ("POST", "/agreement", Own, "accept an agreement"),
    ("POST", "/api/agreement", Own, "accept an agreement"),
    (
        "GET",
        "/agreement/accepted/{id}",
        Own,
        "the caller's acceptance of agreement {id}",
    ),
    (
        "GET",
        "/api/agreement/accepted/{id}",
        Own,
        "the caller's acceptance of agreement {id}",
    ),
    ("GET", "/api/chat/history", Own, "filtered by caller"),
    ("PUT", "/api/chat/history", Own, "filtered by caller"),
    ("DELETE", "/api/chat/history", Own, "filtered by caller"),
    ("GET", "/api/chat/sessions", Own, ""),
    ("POST", "/api/chat/sessions", Own, ""),
    ("POST", "/api/templates", Own, "create a template"),
    ("GET", "/api/templates/mine", Own, ""),
    ("GET", "/api/templates/mine/analytics", Own, ""),
    ("GET", "/api/templates/mine/vendor-profile", Own, ""),
    ("PATCH", "/api/templates/mine/vendor-profile", Own, ""),
    (
        "POST",
        "/api/templates/mine/vendor-profile/onboarding-link",
        Own,
        "",
    ),
    (
        "POST",
        "/api/templates/mine/vendor-profile/onboarding-complete",
        Own,
        "",
    ),
    (
        "GET",
        "/api/templates/{id}/rating/me",
        Own,
        "the caller's rating",
    ),
    (
        "PUT",
        "/api/templates/{id}/rating",
        Own,
        "the caller's rating",
    ),
    (
        "DELETE",
        "/api/templates/{id}/rating",
        Own,
        "the caller's rating",
    ),
    (
        "POST",
        "/api/templates/{slug}/install",
        Own,
        "installs an approved template for the caller",
    ),
    (
        "POST",
        "/api/v1/templates/{slug}/install",
        Own,
        "installs an approved template for the caller",
    ),
    ("GET", "/api/v1/deployments", Sweep, "?project_id="),
    ("GET", "/api/v1/pipes/templates", Own, ""),
    ("POST", "/api/v1/pipes/templates", Own, ""),
    ("GET", "/api/v1/pipes/instances/local", Own, ""),
    ("POST", "/api/v1/pipes/field-match", Own, "pure computation"),
    (
        "POST",
        "/api/v1/handoff/mint/account",
        Own,
        "a handoff for the caller's account",
    ),
    // tokens, keys, signatures
    (
        "POST",
        "/api/v1/handoff/resolve",
        Token,
        "single-use handoff token",
    ),
    (
        "GET",
        "/api/v1/marketplace/install/{purchase_token}",
        Token,
        "purchase token",
    ),
    (
        "GET",
        "/api/v1/marketplace/download/{purchase_token}",
        Token,
        "purchase token",
    ),
    (
        "POST",
        "/api/v1/marketplace/deploy-complete",
        Token,
        "internal service key",
    ),
    (
        "POST",
        "/api/v1/marketplace/payouts/webhook",
        Token,
        "webhook signature",
    ),
    (
        "POST",
        "/api/v1/marketplace/agents/register",
        Token,
        "answers 501",
    ),
    (
        "POST",
        "/api/v1/agent/register",
        Token,
        "internal service key",
    ),
    (
        "POST",
        "/api/v1/agent/login",
        Token,
        "user credentials, then deployment ownership",
    ),
    (
        "POST",
        "/api/v1/agent/link",
        Token,
        "session token, then deployment ownership",
    ),
    // test endpoints: no Casbin grant for regular users (checked live)
    (
        "POST",
        "/test/deploy",
        Admin,
        "echoes the calling HMAC client",
    ),
    (
        "GET",
        "/test/stack_view",
        Admin,
        "User Service connectivity probe",
    ),
    // swept for IDOR below
    ("GET", "/cloud/{id}", Sweep, ""),
    ("PUT", "/cloud/{id}", Sweep, ""),
    ("DELETE", "/cloud/{id}", Sweep, ""),
    ("GET", "/server/{id}", Sweep, ""),
    ("PUT", "/server/{id}", Sweep, ""),
    ("DELETE", "/server/{id}", Sweep, ""),
    ("GET", "/server/{id}/delete-preview", Sweep, ""),
    ("GET", "/server/{id}/ssh-key/public", Sweep, ""),
    ("DELETE", "/server/{id}/ssh-key", Sweep, ""),
    ("POST", "/server/{id}/ssh-key/generate", Sweep, ""),
    ("POST", "/server/{id}/ssh-key/upload", Sweep, ""),
    ("POST", "/server/{id}/ssh-key/validate", Sweep, ""),
    (
        "POST",
        "/server/{id}/ssh-key/authorize-public-key",
        Sweep,
        "",
    ),
    ("POST", "/server/{id}/cloud-firewall", Sweep, ""),
    ("GET", "/server/project/{project_id}", Sweep, ""),
    ("GET", "/server/{server_id}/secrets", Sweep, ""),
    ("GET", "/server/{server_id}/secrets/{name}", Sweep, ""),
    ("PUT", "/server/{server_id}/secrets/{name}", Sweep, ""),
    ("DELETE", "/server/{server_id}/secrets/{name}", Sweep, ""),
    ("PUT", "/client/{id}", Sweep, ""),
    ("PUT", "/client/{id}/disable", Sweep, ""),
    ("PUT", "/client/{id}/enable", Sweep, ""),
    ("PUT", "/rating/{id}", Sweep, ""),
    ("DELETE", "/rating/{id}", Sweep, ""),
    ("PATCH", "/api/chat/sessions/{id}", Sweep, ""),
    ("POST", "/api/chat/sessions/{id}/archive", Sweep, ""),
    ("POST", "/api/chat/sessions/{id}/unarchive", Sweep, ""),
    ("GET", "/api/chat/sessions/{id}/messages", Sweep, ""),
    ("POST", "/api/chat/sessions/{id}/messages", Sweep, ""),
    ("PUT", "/api/chat/sessions/{id}/messages", Sweep, ""),
    ("DELETE", "/api/chat/sessions/{id}", Sweep, ""),
    ("PUT", "/api/templates/{id}", Sweep, ""),
    ("POST", "/api/templates/{id}/submit", Sweep, ""),
    ("POST", "/api/templates/{id}/resubmit", Sweep, ""),
    ("GET", "/api/templates/{id}/reviews", Sweep, ""),
    (
        "GET",
        "/api/templates/{id}/vendor-profile-status",
        Sweep,
        "",
    ),
    ("POST", "/api/templates/{id}/assets/presign", Sweep, ""),
    ("POST", "/api/templates/{id}/assets/finalize", Sweep, ""),
    (
        "POST",
        "/api/templates/{id}/assets/presign-download",
        Sweep,
        "",
    ),
    ("POST", "/api/v1/templates/{id}/assets/presign", Sweep, ""),
    ("POST", "/api/v1/templates/{id}/assets/finalize", Sweep, ""),
    (
        "POST",
        "/api/v1/templates/{id}/assets/presign-download",
        Sweep,
        "",
    ),
    ("GET", "/api/v1/deployments/{id}", Sweep, ""),
    ("POST", "/api/v1/deployments/{id}/force-complete", Sweep, ""),
    ("GET", "/api/v1/deployments/hash/{hash}", Sweep, ""),
    ("GET", "/api/v1/deployments/project/{project_id}", Sweep, ""),
    (
        "GET",
        "/api/v1/deployments/{deployment_hash}/capabilities",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/deployments/{deployment_hash}/events",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/deployments/{deployment_hash}/plan",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/deployments/{deployment_hash}/state",
        Sweep,
        "",
    ),
    ("POST", "/api/v1/commands", Sweep, ""),
    ("GET", "/api/v1/commands/{deployment_hash}", Sweep, ""),
    (
        "GET",
        "/api/v1/commands/{deployment_hash}/{command_id}",
        Sweep,
        "",
    ),
    (
        "POST",
        "/api/v1/commands/{deployment_hash}/{command_id}/cancel",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/agent/deployments/{deployment_hash}",
        Sweep,
        "",
    ),
    ("GET", "/api/v1/agent/project/{project_id}", Sweep, ""),
    (
        "POST",
        "/api/v1/agent/rotate-token/{deployment_hash}",
        Sweep,
        "",
    ),
    ("POST", "/api/v1/agent/commands/enqueue", Sweep, ""),
    ("GET", "/api/v1/agent/audit", Sweep, ""),
    ("POST", "/api/v1/handoff/mint", Sweep, ""),
    ("GET", "/api/v1/pipes/templates/{template_id}", Sweep, ""),
    ("DELETE", "/api/v1/pipes/templates/{template_id}", Sweep, ""),
    ("POST", "/api/v1/pipes/{template_id}/dag/steps", Sweep, ""),
    ("GET", "/api/v1/pipes/{template_id}/dag/steps", Sweep, ""),
    (
        "GET",
        "/api/v1/pipes/{template_id}/dag/steps/{step_id}",
        Sweep,
        "",
    ),
    (
        "PUT",
        "/api/v1/pipes/{template_id}/dag/steps/{step_id}",
        Sweep,
        "",
    ),
    (
        "DELETE",
        "/api/v1/pipes/{template_id}/dag/steps/{step_id}",
        Sweep,
        "",
    ),
    ("POST", "/api/v1/pipes/{template_id}/dag/edges", Sweep, ""),
    ("GET", "/api/v1/pipes/{template_id}/dag/edges", Sweep, ""),
    (
        "DELETE",
        "/api/v1/pipes/{template_id}/dag/edges/{edge_id}",
        Sweep,
        "",
    ),
    (
        "POST",
        "/api/v1/pipes/{template_id}/dag/validate",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/pipes/{template_id}/dag/executions/{execution_id}/steps",
        Sweep,
        "",
    ),
    ("POST", "/api/v1/pipes/instances", Sweep, ""),
    (
        "GET",
        "/api/v1/pipes/instances/{deployment_hash}",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/pipes/instances/detail/{instance_id}",
        Sweep,
        "",
    ),
    ("DELETE", "/api/v1/pipes/instances/{instance_id}", Sweep, ""),
    (
        "PUT",
        "/api/v1/pipes/instances/{instance_id}/status",
        Sweep,
        "",
    ),
    (
        "POST",
        "/api/v1/pipes/instances/{instance_id}/deploy",
        Sweep,
        "",
    ),
    (
        "POST",
        "/api/v1/pipes/instances/{instance_id}/dag/execute",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/pipes/instances/{instance_id}/executions",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/pipes/instances/{instance_id}/stream",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/pipes/instances/{instance_id}/dlq",
        Sweep,
        "",
    ),
    (
        "POST",
        "/api/v1/pipes/instances/{instance_id}/dlq",
        Sweep,
        "",
    ),
    (
        "GET",
        "/api/v1/pipes/instances/{instance_id}/circuit-breaker",
        Sweep,
        "",
    ),
    (
        "PUT",
        "/api/v1/pipes/instances/{instance_id}/circuit-breaker",
        Sweep,
        "",
    ),
    (
        "POST",
        "/api/v1/pipes/instances/{instance_id}/circuit-breaker/failure",
        Sweep,
        "",
    ),
    (
        "POST",
        "/api/v1/pipes/instances/{instance_id}/circuit-breaker/success",
        Sweep,
        "",
    ),
    (
        "POST",
        "/api/v1/pipes/instances/{instance_id}/circuit-breaker/reset",
        Sweep,
        "",
    ),
    ("GET", "/api/v1/pipes/executions/{execution_id}", Sweep, ""),
    (
        "POST",
        "/api/v1/pipes/executions/{execution_id}/replay",
        Sweep,
        "",
    ),
    ("GET", "/api/v1/pipes/dlq/{entry_id}", Sweep, ""),
    ("POST", "/api/v1/pipes/dlq/{entry_id}/retry", Sweep, ""),
    ("DELETE", "/api/v1/pipes/dlq/{entry_id}", Sweep, ""),
    // agent token
    (
        "GET",
        "/api/v1/agent/commands/wait/{deployment_hash}",
        Agent,
        "",
    ),
    ("POST", "/api/v1/agent/commands/report", Agent, ""),
    ("GET", "/api/v1/agent/notifications", Agent, ""),
    ("POST", "/api/v1/agent/audit", Agent, ""),
];

fn table() -> BTreeMap<(String, String), (Kind, &'static str)> {
    let mut map = BTreeMap::new();
    for (method, path, kind, note) in ROUTES {
        let key = (method.to_string(), path.to_string());
        assert!(
            map.insert(key.clone(), (*kind, *note)).is_none(),
            "{:?} is listed twice",
            key
        );
    }
    map
}

// ── Route inventory ──────────────────────────────────────────────────────

/// Every route registered in src/startup.rs, as (METHOD, full path).
fn registered_routes() -> BTreeSet<(String, String)> {
    let root = env!("CARGO_MANIFEST_DIR");
    let startup = std::fs::read_to_string(format!("{root}/src/startup.rs")).unwrap();

    let project_scope = {
        let start = startup.find("fn project_scope").expect("project_scope");
        let end = start + startup[start..].find("\n}").expect("end of project_scope");
        handlers_in(&startup[start..end])
    };
    let run = {
        let start = startup.find("pub async fn run(").expect("run()");
        let end = startup.find("#[cfg(test)]").unwrap_or(startup.len());
        strip_comments(&startup[start..end])
    };

    // Walk run(): track web::scope("...") prefixes by parenthesis depth.
    let mut routes = BTreeSet::new();
    let mut scopes: Vec<(String, usize)> = Vec::new();
    let mut depth = 0usize;
    let bytes = run.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &run[i..];
        if let Some(arg) = literal_call(rest, "web::scope(") {
            scopes.push((arg.0, depth));
            i += arg.1;
            continue;
        }
        if let Some(arg) = literal_call(rest, "project_scope(") {
            let outer: String = scopes.iter().map(|s| s.0.as_str()).collect();
            let prefix = format!("{outer}{}", arg.0);
            for handler in &project_scope {
                let (method, path) = route_of(root, handler);
                routes.insert((method, format!("{prefix}{path}")));
            }
            i += arg.1;
            continue;
        }
        if rest.starts_with(".service(") {
            let inner_start = i + ".service(".len();
            let inner = run[inner_start..].trim_start();
            if inner.starts_with("crate::routes::") || inner.starts_with("routes::") {
                let end = inner
                    .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
                    .unwrap();
                let handler = inner[..end].trim_start_matches("crate::").to_string();
                let prefix: String = scopes.iter().map(|s| s.0.as_str()).collect();
                let (method, path) = route_of(root, &handler);
                routes.insert((method, format!("{prefix}{path}")));
            }
        }
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                while scopes.last().map(|s| depth < s.1).unwrap_or(false) {
                    scopes.pop();
                }
            }
            _ => {}
        }
        i += 1;
    }
    routes
}

fn strip_comments(src: &str) -> String {
    src.lines()
        .map(|l| match l.find("//") {
            Some(pos) => &l[..pos],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `name("literal")` at the start of `s`: returns the literal and the length
/// consumed (up to and including the closing parenthesis).
fn literal_call(s: &str, name: &str) -> Option<(String, usize)> {
    let rest = s.strip_prefix(name)?;
    let rest_trim = rest.trim_start();
    let lead = rest.len() - rest_trim.len();
    let body = rest_trim.strip_prefix('"')?;
    let close = body.find('"')?;
    let after = body[close + 1..].trim_start();
    let after_lead = body[close + 1..].len() - after.len();
    after.strip_prefix(')')?;
    Some((
        body[..close].to_string(),
        name.len() + lead + 1 + close + 1 + after_lead + 1,
    ))
}

fn handlers_in(src: &str) -> Vec<String> {
    src.lines()
        .filter_map(|l| l.trim().strip_prefix(".service("))
        .map(|h| {
            h.trim_end_matches(')')
                .trim_start_matches("crate::")
                .to_string()
        })
        .collect()
}

/// Method and path of a handler such as `routes::server::get::item`, read
/// from the route attribute above `pub async fn item(`.
fn route_of(root: &str, handler: &str) -> (String, String) {
    let parts: Vec<&str> = handler.split("::").skip(1).collect(); // drop "routes"
    let (name, modules) = parts.split_last().unwrap();
    // `routes::health_check` re-exports a handler from the routes root: search
    // all of src/routes for it.
    for depth in (0..=modules.len()).rev() {
        let base = format!("{root}/src/routes/{}", modules[..depth].join("/"));
        let base = base.trim_end_matches('/').to_string();
        let mut files = Vec::new();
        if std::path::Path::new(&format!("{base}.rs")).exists() {
            files.push(format!("{base}.rs"));
        }
        collect_rs(std::path::Path::new(&base), &mut files);
        let mut found = Vec::new();
        for file in &files {
            let src = std::fs::read_to_string(file).unwrap();
            let lines: Vec<&str> = src.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                let t = line.trim_start();
                if t.starts_with(&format!("pub async fn {name}("))
                    || t.starts_with(&format!("pub async fn {name}<"))
                {
                    let attr = lines[n.saturating_sub(15)..n]
                        .iter()
                        .rev()
                        .map(|l| l.trim())
                        .find(|l| {
                            ["#[get(", "#[post(", "#[put(", "#[patch(", "#[delete("]
                                .iter()
                                .any(|p| l.starts_with(p))
                        });
                    if let Some(attr) = attr {
                        let method = attr[2..attr.find('(').unwrap()].to_uppercase();
                        let path = attr.split('"').nth(1).unwrap().to_string();
                        found.push((method, path));
                    }
                }
            }
        }
        if found.len() == 1 {
            return found.pop().unwrap();
        }
        if found.len() > 1 {
            panic!("{handler} is ambiguous: {:?}", found);
        }
    }
    panic!("route attribute for {handler} not found");
}

fn collect_rs(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path.to_string_lossy().to_string());
        }
    }
}

fn is_project_route(path: &str) -> bool {
    ["/project/{", "/api/v1/project/{"]
        .iter()
        .any(|p| path.starts_with(p))
}

fn is_admin_route(path: &str) -> bool {
    path.starts_with("/admin/") || path.starts_with("/api/admin/")
}

fn kind_of(
    table: &BTreeMap<(String, String), (Kind, &'static str)>,
    route: &(String, String),
) -> Option<Kind> {
    if let Some((kind, _)) = table.get(route) {
        return Some(*kind);
    }
    if is_project_route(&route.1) {
        return Some(ProjectSweep);
    }
    if is_admin_route(&route.1) {
        return Some(Admin);
    }
    None
}

#[test]
fn every_route_is_classified() {
    let registered = registered_routes();
    assert!(
        registered.len() > 200,
        "parsed only {} routes - the parser no longer understands src/startup.rs",
        registered.len()
    );
    let table = table();
    let unclassified: Vec<_> = registered
        .iter()
        .filter(|r| kind_of(&table, r).is_none())
        .collect();
    let stale: Vec<_> = table.keys().filter(|k| !registered.contains(*k)).collect();
    assert!(
        unclassified.is_empty() && stale.is_empty(),
        "unclassified routes ({}):\n{}\nlisted but not registered ({}):\n{}",
        unclassified.len(),
        unclassified
            .iter()
            .map(|(m, p)| format!("    (\"{m}\", \"{p}\", ?, \"\"),"))
            .collect::<Vec<_>>()
            .join("\n"),
        stale.len(),
        stale
            .iter()
            .map(|(m, p)| format!("    {m} {p}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

// ── Live checks ──────────────────────────────────────────────────────────

const MISSING_INT: &str = "2147480000";
const MISSING_UUID: &str = "00000000-0000-4000-8000-0000000000ff";
const HASH: &str = "matrix-sweep-hash";

/// User A's resources, by placeholder name.
struct Fixture(BTreeMap<&'static str, String>);

impl Fixture {
    fn fill(&self, template: &str) -> String {
        let mut out = template.to_string();
        for (key, value) in &self.0 {
            out = out.replace(&format!("{{{key}}}"), value);
        }
        out
    }

    /// The same request against resources that do not exist.
    fn missing() -> Self {
        let mut m = BTreeMap::new();
        for key in [
            "project",
            "deployment_id",
            "cloud",
            "server",
            "client",
            "rating",
        ] {
            m.insert(key, MISSING_INT.to_string());
        }
        for key in [
            "session",
            "template",
            "ptemplate",
            "pinstance",
            "pexecution",
            "pdlq",
            "step",
        ] {
            m.insert(key, MISSING_UUID.to_string());
        }
        m.insert("hash", "matrix-missing-hash".to_string());
        m.insert("command", "cmd_matrix_missing".to_string());
        Fixture(m)
    }
}

/// (method, registered path) -> (url, JSON body). Placeholders in braces are
/// User A's resources (see `Fixture`).
fn sweep_requests() -> BTreeMap<(&'static str, &'static str), (&'static str, &'static str)> {
    let rows: &[(&str, &str, &str, &str)] = &[
        ("GET", "/cloud/{id}", "/cloud/{cloud}", ""),
        (
            "PUT",
            "/cloud/{id}",
            "/cloud/{cloud}",
            r#"{"provider":"htz","cloud_token":"intruder","save_token":true,"name":"intruder"}"#,
        ),
        ("DELETE", "/cloud/{id}", "/cloud/{cloud}", ""),
        ("GET", "/server/{id}", "/server/{server}", ""),
        (
            "PUT",
            "/server/{id}",
            "/server/{server}",
            r#"{"name":"intruder","region":"fsn1","server":"cx22","os":"ubuntu-24.04","disk_type":"local"}"#,
        ),
        ("DELETE", "/server/{id}", "/server/{server}", ""),
        (
            "GET",
            "/server/{id}/delete-preview",
            "/server/{server}/delete-preview",
            "",
        ),
        (
            "GET",
            "/server/{id}/ssh-key/public",
            "/server/{server}/ssh-key/public",
            "",
        ),
        (
            "DELETE",
            "/server/{id}/ssh-key",
            "/server/{server}/ssh-key",
            "",
        ),
        (
            "POST",
            "/server/{id}/ssh-key/generate",
            "/server/{server}/ssh-key/generate",
            "",
        ),
        (
            "POST",
            "/server/{id}/ssh-key/upload",
            "/server/{server}/ssh-key/upload",
            r#"{"public_key":"ssh-ed25519 AAAA intruder","private_key":"not-a-real-key"}"#,
        ),
        (
            "POST",
            "/server/{id}/ssh-key/validate",
            "/server/{server}/ssh-key/validate",
            "",
        ),
        (
            "POST",
            "/server/{id}/ssh-key/authorize-public-key",
            "/server/{server}/ssh-key/authorize-public-key",
            r#"{"public_key":"ssh-ed25519 AAAA intruder"}"#,
        ),
        (
            "POST",
            "/server/{id}/cloud-firewall",
            "/server/{server}/cloud-firewall",
            r#"{"public_ports":[{"port":8080,"protocol":"tcp","source":"0.0.0.0/0"}],"private_ports":[]}"#,
        ),
        (
            "GET",
            "/server/project/{project_id}",
            "/server/project/{project}",
            "",
        ),
        (
            "GET",
            "/server/{server_id}/secrets",
            "/server/{server}/secrets",
            "",
        ),
        (
            "GET",
            "/server/{server_id}/secrets/{name}",
            "/server/{server}/secrets/SOME_NAME",
            "",
        ),
        (
            "PUT",
            "/server/{server_id}/secrets/{name}",
            "/server/{server}/secrets/SOME_NAME",
            r#"{"value":"intruder"}"#,
        ),
        (
            "DELETE",
            "/server/{server_id}/secrets/{name}",
            "/server/{server}/secrets/SOME_NAME",
            "",
        ),
        ("PUT", "/client/{id}", "/client/{client}", ""),
        (
            "PUT",
            "/client/{id}/disable",
            "/client/{client}/disable",
            "",
        ),
        ("PUT", "/client/{id}/enable", "/client/{client}/enable", ""),
        (
            "PUT",
            "/rating/{id}",
            "/rating/{rating}",
            r#"{"rate":1,"comment":"intruder"}"#,
        ),
        ("DELETE", "/rating/{id}", "/rating/{rating}", ""),
        (
            "PATCH",
            "/api/chat/sessions/{id}",
            "/api/chat/sessions/{session}",
            r#"{"title":"intruder"}"#,
        ),
        (
            "POST",
            "/api/chat/sessions/{id}/archive",
            "/api/chat/sessions/{session}/archive",
            "",
        ),
        (
            "POST",
            "/api/chat/sessions/{id}/unarchive",
            "/api/chat/sessions/{session}/unarchive",
            "",
        ),
        (
            "GET",
            "/api/chat/sessions/{id}/messages",
            "/api/chat/sessions/{session}/messages",
            "",
        ),
        (
            "POST",
            "/api/chat/sessions/{id}/messages",
            "/api/chat/sessions/{session}/messages",
            r#"{"role":"user","content":"intruder"}"#,
        ),
        (
            "PUT",
            "/api/chat/sessions/{id}/messages",
            "/api/chat/sessions/{session}/messages",
            r#"{"messages":[{"role":"user","content":"intruder"}]}"#,
        ),
        (
            "DELETE",
            "/api/chat/sessions/{id}",
            "/api/chat/sessions/{session}",
            "",
        ),
        (
            "PUT",
            "/api/templates/{id}",
            "/api/templates/{template}",
            r#"{"name":"intruder"}"#,
        ),
        (
            "POST",
            "/api/templates/{id}/submit",
            "/api/templates/{template}/submit",
            "{}",
        ),
        (
            "POST",
            "/api/templates/{id}/resubmit",
            "/api/templates/{template}/resubmit",
            r#"{"version":"9.9.9"}"#,
        ),
        (
            "GET",
            "/api/templates/{id}/reviews",
            "/api/templates/{template}/reviews",
            "",
        ),
        (
            "GET",
            "/api/templates/{id}/vendor-profile-status",
            "/api/templates/{template}/vendor-profile-status",
            "",
        ),
        (
            "POST",
            "/api/templates/{id}/assets/presign",
            "/api/templates/{template}/assets/presign",
            r#"{"filename":"a.tar.gz","sha256":"00","size":1,"content_type":"application/gzip"}"#,
        ),
        (
            "POST",
            "/api/templates/{id}/assets/finalize",
            "/api/templates/{template}/assets/finalize",
            r#"{"bucket":"x","key":"x","filename":"a.tar.gz","sha256":"00","size":1}"#,
        ),
        (
            "POST",
            "/api/templates/{id}/assets/presign-download",
            "/api/templates/{template}/assets/presign-download",
            r#"{"key":"x"}"#,
        ),
        (
            "POST",
            "/api/v1/templates/{id}/assets/presign",
            "/api/v1/templates/{template}/assets/presign",
            r#"{"filename":"a.tar.gz","sha256":"00","size":1,"content_type":"application/gzip"}"#,
        ),
        (
            "POST",
            "/api/v1/templates/{id}/assets/finalize",
            "/api/v1/templates/{template}/assets/finalize",
            r#"{"bucket":"x","key":"x","filename":"a.tar.gz","sha256":"00","size":1}"#,
        ),
        (
            "POST",
            "/api/v1/templates/{id}/assets/presign-download",
            "/api/v1/templates/{template}/assets/presign-download",
            r#"{"key":"x"}"#,
        ),
        (
            "GET",
            "/api/v1/deployments",
            "/api/v1/deployments?project_id={project}",
            "",
        ),
        (
            "GET",
            "/api/v1/deployments/{id}",
            "/api/v1/deployments/{deployment_id}",
            "",
        ),
        (
            "POST",
            "/api/v1/deployments/{id}/force-complete",
            "/api/v1/deployments/{deployment_id}/force-complete",
            "",
        ),
        (
            "GET",
            "/api/v1/deployments/hash/{hash}",
            "/api/v1/deployments/hash/{hash}",
            "",
        ),
        (
            "GET",
            "/api/v1/deployments/project/{project_id}",
            "/api/v1/deployments/project/{project}",
            "",
        ),
        (
            "GET",
            "/api/v1/deployments/{deployment_hash}/capabilities",
            "/api/v1/deployments/{hash}/capabilities",
            "",
        ),
        (
            "GET",
            "/api/v1/deployments/{deployment_hash}/events",
            "/api/v1/deployments/{hash}/events",
            "",
        ),
        (
            "GET",
            "/api/v1/deployments/{deployment_hash}/plan",
            "/api/v1/deployments/{hash}/plan",
            "",
        ),
        (
            "GET",
            "/api/v1/deployments/{deployment_hash}/state",
            "/api/v1/deployments/{hash}/state",
            "",
        ),
        (
            "POST",
            "/api/v1/commands",
            "/api/v1/commands",
            r#"{"deployment_hash":"{hash}","command_type":"health","parameters":{"app_code":"all"}}"#,
        ),
        (
            "GET",
            "/api/v1/commands/{deployment_hash}",
            "/api/v1/commands/{hash}",
            "",
        ),
        (
            "GET",
            "/api/v1/commands/{deployment_hash}/{command_id}",
            "/api/v1/commands/{hash}/{command}",
            "",
        ),
        (
            "POST",
            "/api/v1/commands/{deployment_hash}/{command_id}/cancel",
            "/api/v1/commands/{hash}/{command}/cancel",
            "",
        ),
        (
            "GET",
            "/api/v1/agent/deployments/{deployment_hash}",
            "/api/v1/agent/deployments/{hash}",
            "",
        ),
        (
            "GET",
            "/api/v1/agent/project/{project_id}",
            "/api/v1/agent/project/{project}",
            "",
        ),
        (
            "POST",
            "/api/v1/agent/rotate-token/{deployment_hash}",
            "/api/v1/agent/rotate-token/{hash}",
            "",
        ),
        (
            "POST",
            "/api/v1/agent/commands/enqueue",
            "/api/v1/agent/commands/enqueue",
            r#"{"deployment_hash":"{hash}","command_type":"health","parameters":{"app_code":"all"}}"#,
        ),
        (
            "GET",
            "/api/v1/agent/audit",
            "/api/v1/agent/audit?installation_hash={hash}",
            "",
        ),
        (
            "POST",
            "/api/v1/handoff/mint",
            "/api/v1/handoff/mint",
            r#"{"deployment_hash":"{hash}"}"#,
        ),
        (
            "GET",
            "/api/v1/pipes/templates/{template_id}",
            "/api/v1/pipes/templates/{ptemplate}",
            "",
        ),
        (
            "DELETE",
            "/api/v1/pipes/templates/{template_id}",
            "/api/v1/pipes/templates/{ptemplate}",
            "",
        ),
        (
            "POST",
            "/api/v1/pipes/{template_id}/dag/steps",
            "/api/v1/pipes/{ptemplate}/dag/steps",
            r#"{"name":"intruder","step_type":"source","config":{}}"#,
        ),
        (
            "GET",
            "/api/v1/pipes/{template_id}/dag/steps",
            "/api/v1/pipes/{ptemplate}/dag/steps",
            "",
        ),
        (
            "GET",
            "/api/v1/pipes/{template_id}/dag/steps/{step_id}",
            "/api/v1/pipes/{ptemplate}/dag/steps/{step}",
            "",
        ),
        (
            "PUT",
            "/api/v1/pipes/{template_id}/dag/steps/{step_id}",
            "/api/v1/pipes/{ptemplate}/dag/steps/{step}",
            r#"{"name":"intruder"}"#,
        ),
        (
            "DELETE",
            "/api/v1/pipes/{template_id}/dag/steps/{step_id}",
            "/api/v1/pipes/{ptemplate}/dag/steps/{step}",
            "",
        ),
        (
            "POST",
            "/api/v1/pipes/{template_id}/dag/edges",
            "/api/v1/pipes/{ptemplate}/dag/edges",
            r#"{"from_step_id":"{step}","to_step_id":"{step}"}"#,
        ),
        (
            "GET",
            "/api/v1/pipes/{template_id}/dag/edges",
            "/api/v1/pipes/{ptemplate}/dag/edges",
            "",
        ),
        (
            "DELETE",
            "/api/v1/pipes/{template_id}/dag/edges/{edge_id}",
            "/api/v1/pipes/{ptemplate}/dag/edges/{step}",
            "",
        ),
        (
            "POST",
            "/api/v1/pipes/{template_id}/dag/validate",
            "/api/v1/pipes/{ptemplate}/dag/validate",
            "",
        ),
        (
            "GET",
            "/api/v1/pipes/{template_id}/dag/executions/{execution_id}/steps",
            "/api/v1/pipes/{ptemplate}/dag/executions/{pexecution}/steps",
            "",
        ),
        (
            "POST",
            "/api/v1/pipes/instances",
            "/api/v1/pipes/instances",
            r#"{"deployment_hash":"{hash}","source_container":"intruder","target_container":"intruder"}"#,
        ),
        (
            "GET",
            "/api/v1/pipes/instances/{deployment_hash}",
            "/api/v1/pipes/instances/{hash}",
            "",
        ),
        (
            "GET",
            "/api/v1/pipes/instances/detail/{instance_id}",
            "/api/v1/pipes/instances/detail/{pinstance}",
            "",
        ),
        (
            "DELETE",
            "/api/v1/pipes/instances/{instance_id}",
            "/api/v1/pipes/instances/{pinstance}",
            "",
        ),
        (
            "PUT",
            "/api/v1/pipes/instances/{instance_id}/status",
            "/api/v1/pipes/instances/{pinstance}/status",
            r#"{"status":"paused"}"#,
        ),
        (
            "POST",
            "/api/v1/pipes/instances/{instance_id}/deploy",
            "/api/v1/pipes/instances/{pinstance}/deploy",
            r#"{"deployment_hash":"{hash}"}"#,
        ),
        (
            "POST",
            "/api/v1/pipes/instances/{instance_id}/dag/execute",
            "/api/v1/pipes/instances/{pinstance}/dag/execute",
            r#"{"input_data":{}}"#,
        ),
        (
            "GET",
            "/api/v1/pipes/instances/{instance_id}/executions",
            "/api/v1/pipes/instances/{pinstance}/executions",
            "",
        ),
        (
            "GET",
            "/api/v1/pipes/instances/{instance_id}/stream",
            "/api/v1/pipes/instances/{pinstance}/stream",
            "",
        ),
        (
            "GET",
            "/api/v1/pipes/instances/{instance_id}/dlq",
            "/api/v1/pipes/instances/{pinstance}/dlq",
            "",
        ),
        (
            "POST",
            "/api/v1/pipes/instances/{instance_id}/dlq",
            "/api/v1/pipes/instances/{pinstance}/dlq",
            r#"{"error":"intruder"}"#,
        ),
        (
            "GET",
            "/api/v1/pipes/instances/{instance_id}/circuit-breaker",
            "/api/v1/pipes/instances/{pinstance}/circuit-breaker",
            "",
        ),
        (
            "PUT",
            "/api/v1/pipes/instances/{instance_id}/circuit-breaker",
            "/api/v1/pipes/instances/{pinstance}/circuit-breaker",
            r#"{"failure_threshold":1}"#,
        ),
        (
            "POST",
            "/api/v1/pipes/instances/{instance_id}/circuit-breaker/failure",
            "/api/v1/pipes/instances/{pinstance}/circuit-breaker/failure",
            "",
        ),
        (
            "POST",
            "/api/v1/pipes/instances/{instance_id}/circuit-breaker/success",
            "/api/v1/pipes/instances/{pinstance}/circuit-breaker/success",
            "",
        ),
        (
            "POST",
            "/api/v1/pipes/instances/{instance_id}/circuit-breaker/reset",
            "/api/v1/pipes/instances/{pinstance}/circuit-breaker/reset",
            "",
        ),
        (
            "GET",
            "/api/v1/pipes/executions/{execution_id}",
            "/api/v1/pipes/executions/{pexecution}",
            "",
        ),
        (
            "POST",
            "/api/v1/pipes/executions/{execution_id}/replay",
            "/api/v1/pipes/executions/{pexecution}/replay",
            "",
        ),
        (
            "GET",
            "/api/v1/pipes/dlq/{entry_id}",
            "/api/v1/pipes/dlq/{pdlq}",
            "",
        ),
        (
            "POST",
            "/api/v1/pipes/dlq/{entry_id}/retry",
            "/api/v1/pipes/dlq/{pdlq}/retry",
            "",
        ),
        (
            "DELETE",
            "/api/v1/pipes/dlq/{entry_id}",
            "/api/v1/pipes/dlq/{pdlq}",
            "",
        ),
    ];
    let mut map = BTreeMap::new();
    for (method, path, url, body) in rows {
        assert!(
            map.insert((*method, *path), (*url, *body)).is_none(),
            "{method} {path} twice"
        );
    }
    map
}

/// Routes whose answer for another user's resource differs from the answer
/// for a missing one, accepted because the id is a random UUID: telling
/// "exists" from "missing" needs the id, which is the secret.
const LEAK_ALLOWED: &[(&str, &str, &str)] = &[
    (
        "PUT",
        "/api/templates/{id}",
        "creator routes answer 403 for another creator's template",
    ),
    (
        "POST",
        "/api/templates/{id}/resubmit",
        "creator routes answer 403",
    ),
    (
        "GET",
        "/api/templates/{id}/reviews",
        "creator routes answer 403",
    ),
    (
        "GET",
        "/api/templates/{id}/vendor-profile-status",
        "creator routes answer 403",
    ),
    (
        "POST",
        "/api/templates/{id}/assets/presign",
        "creator routes answer 403",
    ),
    (
        "POST",
        "/api/templates/{id}/assets/finalize",
        "creator routes answer 403",
    ),
    (
        "POST",
        "/api/templates/{id}/assets/presign-download",
        "creator routes answer 403",
    ),
    (
        "POST",
        "/api/v1/templates/{id}/assets/presign",
        "creator routes answer 403",
    ),
    (
        "POST",
        "/api/v1/templates/{id}/assets/finalize",
        "creator routes answer 403",
    ),
    (
        "POST",
        "/api/v1/templates/{id}/assets/presign-download",
        "creator routes answer 403",
    ),
    (
        "POST",
        "/api/templates/{id}/submit",
        "creator routes answer 403",
    ),
    (
        "GET",
        "/api/v1/pipes/dlq/{entry_id}",
        "names the parent instance when the entry exists",
    ),
    (
        "POST",
        "/api/v1/pipes/dlq/{entry_id}/retry",
        "names the parent instance",
    ),
    (
        "DELETE",
        "/api/v1/pipes/dlq/{entry_id}",
        "names the parent instance",
    ),
    (
        "GET",
        "/api/v1/pipes/executions/{execution_id}",
        "names the parent instance",
    ),
    (
        "POST",
        "/api/v1/pipes/executions/{execution_id}/replay",
        "names the parent instance",
    ),
    (
        "GET",
        "/api/v1/pipes/instances/{instance_id}/stream",
        "answers 403 for another user's instance",
    ),
];

#[test]
fn every_sweep_route_has_a_request() {
    let requests = sweep_requests();
    let sweep: BTreeSet<(&str, &str)> = ROUTES
        .iter()
        .filter(|r| r.2 == Sweep)
        .map(|r| (r.0, r.1))
        .collect();
    let keys: BTreeSet<(&str, &str)> = requests.keys().copied().collect();
    let without: Vec<_> = sweep.difference(&keys).collect();
    let extra: Vec<_> = keys.difference(&sweep).collect();
    assert!(
        without.is_empty() && extra.is_empty(),
        "SWEEP routes without a request: {:?}\nrequests for routes that are not SWEEP: {:?}",
        without,
        extra
    );
}

async fn insert_returning(pool: &sqlx::PgPool, sql: &str, user: &str) -> String {
    let row: (String,) = sqlx::query_as(sql)
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("fixture failed: {e}\n{sql}"));
    row.0
}

async fn seed(pool: &sqlx::PgPool) -> Fixture {
    let a = common::USER_A_ID;
    let project = common::create_test_project(pool, a).await;
    let deployment_id = common::create_test_deployment(pool, a, project, HASH).await;
    let cloud = common::create_test_cloud(pool, a, "a-matrix", "htz").await;
    let server = common::create_test_server(pool, a, project, "none", None).await;
    let client = insert_returning(pool, "INSERT INTO client (user_id, secret, created_at, updated_at) VALUES ($1, 'secret123', NOW(), NOW()) RETURNING id::text", a).await;
    let rating = insert_returning(pool, "INSERT INTO rating (user_id, obj_id, rate, comment, category, hidden, created_at, updated_at) VALUES ($1, 1, 5, 'great', 'application', false, now(), now()) RETURNING id::text", a).await;
    let session = insert_returning(pool, "INSERT INTO chat_sessions (user_id, title, messages_encrypted) VALUES ($1, 'mine', '') RETURNING id::text", a).await;
    let template = insert_returning(pool, "INSERT INTO stack_template (creator_user_id, name, slug, status) VALUES ($1, 'A template', 'matrix-a-template', 'draft') RETURNING id::text", a).await;
    let ptemplate = insert_returning(pool, "INSERT INTO pipe_templates (name, source_app_type, source_endpoint, target_app_type, target_endpoint, field_mapping, is_public, created_by) VALUES ('a-pipe', 'app-a', '{}'::jsonb, 'app-b', '{}'::jsonb, '{}'::jsonb, false, $1) RETURNING id::text", a).await;
    let pinstance = insert_returning(pool, &format!("INSERT INTO pipe_instances (deployment_hash, source_container, status, created_by) VALUES ('{HASH}', 'my-app', 'active', $1) RETURNING id::text"), a).await;
    let pexecution = insert_returning(pool, &format!("INSERT INTO pipe_executions (pipe_instance_id, deployment_hash, created_by) VALUES ('{pinstance}', '{HASH}', $1) RETURNING id::text"), a).await;
    let pdlq = insert_returning(pool, &format!("INSERT INTO dead_letter_queue (pipe_instance_id, error, created_by) VALUES ('{pinstance}', 'boom', $1) RETURNING id::text"), a).await;
    let step = insert_returning(pool, &format!("INSERT INTO pipe_dag_steps (pipe_template_id, name, step_type, config) SELECT '{ptemplate}', 'a-step', 'source', '{{}}'::jsonb WHERE $1 IS NOT NULL RETURNING id::text"), a).await;
    let command = insert_returning(pool, &format!("INSERT INTO commands (command_id, deployment_hash, type, status, parameters, created_by, created_at) VALUES ('cmd_matrix_a', '{HASH}', 'health', 'queued', '{{}}'::jsonb, $1, NOW()) RETURNING command_id"), a).await;

    let mut m = BTreeMap::new();
    m.insert("project", project.to_string());
    m.insert("deployment_id", deployment_id.to_string());
    m.insert("hash", HASH.to_string());
    m.insert("cloud", cloud.to_string());
    m.insert("server", server.to_string());
    m.insert("client", client);
    m.insert("rating", rating);
    m.insert("session", session);
    m.insert("template", template);
    m.insert("ptemplate", ptemplate);
    m.insert("pinstance", pinstance);
    m.insert("pexecution", pexecution);
    m.insert("pdlq", pdlq);
    m.insert("step", step);
    m.insert("command", command);
    Fixture(m)
}

/// Everything User A owns, as text, to compare before and after the sweep.
async fn snapshot(pool: &sqlx::PgPool) -> String {
    let tables = [
        ("project", "user_id"),
        ("deployment", "user_id"),
        ("cloud", "user_id"),
        ("server", "user_id"),
        ("client", "user_id"),
        ("rating", "user_id"),
        ("chat_sessions", "user_id"),
        ("stack_template", "creator_user_id"),
        ("pipe_templates", "created_by"),
        ("pipe_instances", "created_by"),
        ("pipe_executions", "created_by"),
        ("dead_letter_queue", "created_by"),
        ("commands", "created_by"),
    ];
    let mut out = String::new();
    for (table, owner) in tables {
        let sql = format!(
            "SELECT COALESCE(json_agg(t ORDER BY t::text)::text, '[]') FROM {table} t WHERE {owner} = $1"
        );
        let row: (String,) = sqlx::query_as(&sql)
            .bind(common::USER_A_ID)
            .fetch_one(pool)
            .await
            .unwrap_or_else(|e| panic!("snapshot {table}: {e}"));
        out.push_str(&format!("{table}: {}\n", row.0));
    }
    let steps: (String,) = sqlx::query_as(
        "SELECT COALESCE(json_agg(t ORDER BY t::text)::text, '[]') FROM pipe_dag_steps t",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    out.push_str(&format!("pipe_dag_steps: {}\n", steps.0));
    out
}

async fn call(
    client: &reqwest::Client,
    base: &str,
    method: &str,
    url: &str,
    body: &str,
    token: &str,
) -> (u16, String) {
    let mut req = client
        .request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
            format!("{base}{url}"),
        )
        .header("Authorization", format!("Bearer {token}"));
    if !body.is_empty() {
        req = req
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }
    let resp = req.send().await.expect("request failed");
    let status = resp.status().as_u16();
    (status, resp.text().await.unwrap_or_default())
}

#[tokio::test]
async fn other_user_gets_the_same_answer_as_for_a_missing_resource() {
    // User Service knows no installation for either user, so a hash that is
    // not in Stacker is simply not found (instead of failing to connect).
    let user_service = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::path("/api/1.0/installations"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "_items": [], "_meta": { "total": 0 } })),
        )
        .mount(&user_service)
        .await;
    wiremock::Mock::given(wiremock::matchers::path_regex(
        "^/install/by-deployment-hash/.*",
    ))
    .respond_with(wiremock::ResponseTemplate::new(404))
    .mount(&user_service)
    .await;
    let Some(app) = common::spawn_app_two_users_with_user_service(&user_service.uri()).await else {
        return;
    };
    let fixture = seed(&app.db_pool).await;
    let missing = Fixture::missing();
    let before = snapshot(&app.db_pool).await;

    let client = reqwest::Client::new();
    let mut failures = Vec::new();
    for ((method, path), (url, body)) in sweep_requests() {
        let (missing_status, missing_body) = call(
            &client,
            &app.address,
            method,
            &missing.fill(url),
            &missing.fill(body),
            common::USER_B_TOKEN,
        )
        .await;
        let (status, answer) = call(
            &client,
            &app.address,
            method,
            &fixture.fill(url),
            &fixture.fill(body),
            common::USER_B_TOKEN,
        )
        .await;
        let short = |s: &str| s.chars().take(200).collect::<String>();
        if status != 404 && status != 403 {
            failures.push(format!(
                "IDOR  {method} {path}: other user got {status} {}",
                short(&answer)
            ));
        } else if (status, &answer) != (missing_status, &missing_body)
            && !LEAK_ALLOWED
                .iter()
                .any(|(m, p, _)| *m == method && *p == path)
        {
            failures.push(format!(
                "LEAK  {method} {path}: other user got {status} {}, missing got {missing_status} {}",
                short(&answer),
                short(&missing_body)
            ));
        }
    }

    let after = snapshot(&app.db_pool).await;
    if before != after {
        for (b, a) in before.lines().zip(after.lines()) {
            if b != a {
                failures.push(format!("DATA  changed:\n  before {b}\n  after  {a}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} route(s) let another user through:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn fill_any(path: &str) -> String {
    let mut out = String::new();
    let mut in_param = false;
    for c in path.chars() {
        match c {
            '{' => {
                in_param = true;
                out.push('1');
            }
            '}' => in_param = false,
            _ if in_param => {}
            _ => out.push(c),
        }
    }
    out
}

#[tokio::test]
async fn regular_user_is_refused_on_admin_routes() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let table = table();
    let client = reqwest::Client::new();
    let mut failures = Vec::new();
    for route in registered_routes() {
        if kind_of(&table, &route) != Some(Admin) {
            continue;
        }
        let (method, path) = &route;
        let (status, answer) = call(
            &client,
            &app.address,
            method,
            &fill_any(path),
            "{}",
            common::USER_B_TOKEN,
        )
        .await;
        if status != 403 && status != 401 {
            failures.push(format!(
                "{method} {path}: regular user got {status} {}",
                answer.chars().take(200).collect::<String>()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "admin routes open to a regular user:\n{}",
        failures.join("\n")
    );
}

#[tokio::test]
async fn agent_routes_refuse_a_user_session() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();
    let mut failures = Vec::new();
    for (method, path, kind, _) in ROUTES {
        if *kind != Agent {
            continue;
        }
        let url = fill_any(path).replace("/1", &format!("/{HASH}"));
        let (status, answer) = call(
            &client,
            &app.address,
            method,
            &url,
            "{}",
            common::USER_B_TOKEN,
        )
        .await;
        // 500 "Missing expected request extension data": the handler needs an
        // agent identity and a user session does not provide one. Not a
        // proper refusal, but nothing runs.
        let refused = status == 401
            || status == 403
            || (status == 500 && answer.contains("Missing expected request extension data"));
        if !refused {
            failures.push(format!(
                "{method} {path}: a user session got {status} {}",
                answer.chars().take(200).collect::<String>()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "agent routes accept a user session:\n{}",
        failures.join("\n")
    );
}
