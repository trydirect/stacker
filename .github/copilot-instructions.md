# Stacker - AI Coding Assistant Instructions

## Project Overview
Stacker is a Rust/Actix-web API service that enables users to build and deploy Docker-based application stacks to cloud providers via the TryDirect API. Core responsibilities: OAuth authentication, project/cloud/deployment management, API client management, and rating systems.

## Marketplace (new)
- Marketplace tables live in **Stacker DB**; approved templates are exposed via `/api/templates` (public) and `/api/admin/templates` (admin).
- **TryDirect user service** stays in its own DB. We ship helper migrations in `migrations_for_trydirect/` to add `marketplace_template_id`, `is_from_marketplace`, `template_version` to its `stack` table—move them manually to that repo.
- Project model now has `source_template_id: Option<Uuid>` and `template_version: Option<String>` for provenance.
- Marketplace models use optional fields for nullable DB columns (e.g., `view_count`, `deploy_count`, `created_at`, `updated_at`, `average_rating`). Keep SQLx queries aligned with these Option types.
- Run `sqlx migrate run` then `cargo sqlx prepare --workspace` whenever queries change; SQLX_OFFLINE relies on the `.sqlx` cache.

## Actix/JsonResponse patterns (important)
- `JsonResponse::build().ok(..)` returns `web::Json<...>` (Responder). Error helpers (`bad_request`, `not_found`, etc.) return `actix_web::Error`.
- In handlers returning `Result<web::Json<_>>`, return errors as `Err(JsonResponse::build().bad_request(...))`; do **not** wrap errors in `Ok(...)`.
- Parse path IDs to `Uuid` early and propagate `ErrorBadRequest` on parse failure.
## Architecture Essentials

### Request Flow Pattern
All routes follow **Actix-web scoped routing** with **OAuth + HMAC authentication middleware**:
1. HTTP request → `middleware/authentication` (OAuth, HMAC, or anonymous)
2. → `middleware/authorization` (Casbin-based ACL rules)
3. → Route handler → Database operation → `JsonResponse` helper

### Authentication Methods (Multi-strategy)
- **OAuth**: External TryDirect service via `auth_url` (configuration.yaml)
- **HMAC**: API clients sign requests with `api_secret` and `api_key`
- **Anonymous**: Limited read-only endpoints
See: [src/middleware/authentication](src/middleware/authentication)

### Authorization: Casbin ACL Rules
**Critical**: Every new endpoint requires `casbin` rules in migrations. Rules define subject (user/admin/client), action (read/write), resource.
- Base rules: [migrations/20240128174529_casbin_rule.up.sql](migrations/20240128174529_casbin_rule.up.sql) (creates table)
- Initial permissions: [migrations/20240401103123_casbin_initial_rules.up.sql](migrations/20240401103123_casbin_initial_rules.up.sql)
- Feature-specific updates: e.g., [migrations/20240412141011_casbin_user_rating_edit.up.sql](migrations/20240412141011_casbin_user_rating_edit.up.sql)

**GOTCHA: Forget Casbin rules → endpoint returns 403 even if code is correct.**

**Example of this gotcha:**

You implement a new endpoint `GET /client` to list user's clients with perfect code:
```rust
#[get("")]
pub async fn list_handler(
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::client::fetch_by_user(pg_pool.get_ref(), &user.id)
        .await
        .map(|clients| JsonResponse::build().set_list(clients).ok("OK"))
}
```

You register it in `startup.rs`:
```rust
.service(
    web::scope("/client")
        .service(routes::client::list_handler)  // ✓ Registered
        .service(routes::client::add_handler)
)
```

You test it:
```bash
curl -H "Authorization: Bearer <valid_token>" http://localhost:8000/client
# Response: 403 Forbidden ❌ 
# But code looks correct!
```

**What happened?** The authentication succeeded (you got a valid user), but authorization failed. Casbin found **no rule** allowing your role to GET `/client`. 

Looking at [migrations/20240401103123_casbin_initial_rules.up.sql](migrations/20240401103123_casbin_initial_rules.up.sql), you can see:
- ✅ Line 10: `p, group_admin, /client, POST` - admins can create
- ✅ Lines 17-19: `p, group_user, /client/:id, *` - users can update by ID
- ❌ **Missing**: `p, group_user, /client, GET` 

The request flow was:
1. ✅ **Authentication**: Bearer token validated → user has role `group_user`
2. ❌ **Authorization**: Casbin checks: "Does `group_user` have permission for `GET /client`?"
   - Query DB: `SELECT * FROM casbin_rule WHERE v0='group_user' AND v1='/client' AND v2='GET'`
   - Result: **No matching rule** → **403 Forbidden**
3. ❌ Route handler never executed

**The fix:** Add Casbin rule in a new migration:
```sql
-- migrations/20250101000000_add_client_list_rule.up.sql
INSERT INTO public.casbin_rule (ptype, v0, v1, v2) 
VALUES ('p', 'group_user', '/client', 'GET');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2) 
VALUES ('p', 'group_admin', '/client', 'GET');
```

Then run: `sqlx migrate run`

Now the test passes:
```bash
curl -H "Authorization: Bearer <valid_token>" http://localhost:8000/client
# Response: 200 OK ✓
```

### Full Authentication Flow (Detailed)

**Request sequence:**
1. HTTP request arrives
2. **Authentication Middleware** (`manager_middleware.rs`) tries in order:
   - `try_oauth()` → Bearer token → fetch user from TryDirect OAuth service → `Arc<User>` + role to extensions
   - `try_hmac()` → `stacker-id` + `stacker-hash` headers → verify HMAC-SHA256 signature → `Arc<User>` from DB
   - `anonym()` → set subject = `"anonym"` (fallback)
3. **Authorization Middleware** (Casbin) checks:
   - Reads `subject` (user.role or "anonym") from extensions
   - Reads `object` (request path, e.g., `/client`) and `action` (HTTP method, e.g., GET)
   - Matches against rules in `casbin_rule` table: `g(subject, policy_subject) && keyMatch2(path, policy_path) && method == policy_method`
   - Example rule: `p, group_user, /client, GET` means any subject in role `group_user` can GET `/client`
   - If no match → returns 403 Forbidden
4. Route handler executes with `user: web::ReqData<Arc<models::User>>` injected

**Three authentication strategies:**

**OAuth (Highest Priority)**
```
Header: Authorization: Bearer {token}
→ Calls TryDirect auth_url with Bearer token
→ Returns User { id, role, ... }
→ Sets subject = user.role (e.g., "group_user", "group_admin")
```
See: [src/middleware/authentication/method/f_oauth.rs](src/middleware/authentication/method/f_oauth.rs)

**HMAC (Second Priority)**
```
Headers: 
  stacker-id: {client_id}
  stacker-hash: {sha256_hash_of_body}
→ Looks up client in DB by id
→ Verifies HMAC-SHA256(body, client.secret) == header hash
→ User = { id: client.user_id, role: "client" }
→ Sets subject = "client" (API client authentication)
```
See: [src/middleware/authentication/method/f_hmac.rs](src/middleware/authentication/method/f_hmac.rs)

**Anonymous (Fallback)**
```
No auth headers
→ Sets subject = "anonym"
→ Can only access endpoints with Casbin rule: p, group_anonymous, {path}, {method}
```
See: [src/middleware/authentication/method/f_anonym.rs](src/middleware/authentication/method/f_anonym.rs)

**Casbin Role Hierarchy:**
```
Individual users/clients inherit permissions from role groups:
- "admin_petru" → group_admin → group_anonymous
- "user_alice" → group_user → group_anonymous
- "anonym" → group_anonymous
```
This means an `admin_petru` request can access any endpoint allowed for `group_admin`, `group_user`, or `group_anonymous`.

## Core Components & Data Models

### External Service Integration Rule ⭐ **CRITICAL**
**All communication with external services (User Service, Payment Service, etc.) MUST go through connectors in `src/connectors/`.**

This rule ensures:
- **Independence**: Stacker works without external services (mock connectors used)
- **Testability**: Test routes without calling external APIs
- **Replaceability**: Swap implementations without changing routes
- **Clear separation**: Routes never know HTTP/AMQP details

### Connector Architecture Pattern

**1. Define Trait** — `src/connectors/{service}.rs`:
```rust
#[async_trait::async_trait]
pub trait UserServiceConnector: Send + Sync {
    async fn create_stack_from_template(
        &self,
        template_id: &Uuid,
        user_id: &str,
        template_version: &str,
        name: &str,
        stack_definition: serde_json::Value,
    ) -> Result<StackResponse, ConnectorError>;
}
```

**2. Implement HTTP Client** — Same file:
```rust
pub struct UserServiceClient {
    base_url: String,
    http_client: reqwest::Client,
    auth_token: Option<String>,
    retry_attempts: usize,
}

#[async_trait::async_trait]
impl UserServiceConnector for UserServiceClient {
    async fn create_stack_from_template(...) -> Result<StackResponse, ConnectorError> {
        // HTTP request logic with retries, error handling
    }
}
```

**3. Provide Mock for Tests** — Same file (gated with `#[cfg(test)]`):
```rust
pub mod mock {
    pub struct MockUserServiceConnector;
    
    #[async_trait::async_trait]
    impl UserServiceConnector for MockUserServiceConnector {
        async fn create_stack_from_template(...) -> Result<StackResponse, ConnectorError> {
            // Return mock data without HTTP call
        }
    }
}
```

**4. Inject into Routes** — Via `web::Data` in [src/startup.rs](src/startup.rs):
```rust
let user_service_connector: Arc<dyn UserServiceConnector> = if enabled {
    Arc::new(UserServiceClient::new(config))
} else {
    Arc::new(MockUserServiceConnector)  // Use mock in tests
};
let user_service_connector = web::Data::new(user_service_connector);
// app_data(...).app_data(user_service_connector.clone())
```

**5. Use in Handlers** — Routes never call HTTP directly:
```rust
pub async fn deploy_handler(
    connector: web::Data<Arc<dyn UserServiceConnector>>,
) -> Result<impl Responder> {
    // Route logic is pure—doesn't care if it's HTTP, mock, or future gRPC
    connector.create_stack_from_template(...).await?;
    Ok(JsonResponse::build().ok("Deployed"))
}
```

### Configuration
Connectors configured in `configuration.yaml`:
```yaml
connectors:
  user_service:
    enabled: true
    base_url: "https://dev.try.direct/server/user"
    timeout_secs: 10
    retry_attempts: 3
  payment_service:
    enabled: false
    base_url: "http://localhost:8000"
```

### Supported Connectors
| Service | File | Trait | HTTP Client | Purpose |
|---------|------|-------|-------------|---------|
| User Service | `connectors/user_service.rs` | `UserServiceConnector` | `UserServiceClient` | Create/fetch stacks, deployments |
| Payment Service | `connectors/payment_service.rs` | `PaymentServiceConnector` | `PaymentServiceClient` | (Future) Process payments |
| RabbitMQ Events | `events/publisher.rs` | - | - | (Future) Async notifications |

### Adding a New Connector

1. Create `src/connectors/{service}.rs` with trait, client, and mock
2. Export in `src/connectors/mod.rs`
3. Add config to `src/connectors/config.rs`
4. Add to `ConnectorConfig` struct in `configuration.rs`
5. Initialize and inject in `startup.rs`
6. Update `configuration.yaml` with defaults

---

## Core Components & Data Models

### Domains
- **Project**: User's stack definition (apps, containers, metadata)
- **Cloud**: Cloud provider credentials (AWS, DO, Hetzner, etc.)
- **Server**: Cloud instances launched from projects
- **Rating**: User feedback on projects (public catalog)
- **Client**: API client credentials (api_key, api_secret) for external apps
- **Deployment**: Deployment status & history
- **Agreement**: User acceptance of terms/conditions

Key models: [src/models](src/models)

### Database (PostgreSQL + SQLx)
- **Connection pooling**: `PgPool` injected via `web::Data` in handlers
- **Queries**: Custom SQL in [src/db](src/db) (no ORM), executed with SQLx macros
- **Migrations**: Use `sqlx migrate run` (command in [Makefile](Makefile))
- **Offline compilation**: `sqlx` configured for `offline` mode; use `cargo sqlx prepare` if changing queries

Example handler pattern:
```rust
#[get("/{id}")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::internal_server_error(err.to_string()))
        .and_then(|project| match project { ... })
}
```

## API Patterns & Conventions

### Response Format (`JsonResponse` helper)
```rust
JsonResponse::build()
    .set_item(Some(item))
    .set_list(vec![...])
    .ok("OK")  // or .error("msg", HttpStatusCode)
```

### Route Organization
Routes grouped by domain scope in [src/routes](src/routes):
- `/client` - API client CRUD
- `/project` - Stack definition CRUD + `/compose` (Docker) + `/deploy` (to cloud)
- `/cloud` - Cloud credentials CRUD
- `/rating` - Project ratings
- `/admin/*` - Admin-only endpoints (authorization enforced)
- `/agreement` - Terms/conditions

### Input Validation
Forms defined in [src/forms](src/forms). Use `serde_valid` for schema validation (e.g., `#[validate]` attributes).

## Development Workflow

### Setup & Builds
```bash
# Database: Start Docker containers
docker-compose up -d

# Migrations: Apply schema changes
sqlx migrate run

# Development server
make dev  # cargo run with tracing

# Testing
make test [TESTS=path::to::test]  # Single-threaded, capture output

# Code quality
make style-check  # rustfmt --all -- --check
make lint         # clippy with -D warnings
```

### Adding New Endpoints

**Example: Add GET endpoint to list user's clients**

1. **Route handler** — Create [src/routes/client/list.rs](src/routes/client/list.rs):
```rust
use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;

#[tracing::instrument(name = "List user clients.")]
#[get("")]
pub async fn list_handler(
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::client::fetch_by_user(pg_pool.get_ref(), &user.id)
        .await
        .map_err(|err| JsonResponse::<Vec<models::Client>>::build().internal_server_error(err))
        .map(|clients| JsonResponse::build().set_list(clients).ok("OK"))
}
```

2. **Database query** — Add to [src/db/client.rs](src/db/client.rs):
```rust
pub async fn fetch_by_user(pool: &PgPool, user_id: &String) -> Result<Vec<models::Client>, String> {
    let query_span = tracing::info_span!("Fetching clients by user");
    sqlx::query_as!(
        models::Client,
        r#"
        SELECT id, user_id, secret 
        FROM client 
        WHERE user_id = $1
        "#,
        user_id,
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch clients: {:?}", err);
        "Internal Server Error".to_string()
    })
}
```

3. **Export handler** — Update [src/routes/client/mod.rs](src/routes/client/mod.rs):
```rust
mod add;
mod list;  // Add this
mod disable;
mod enable;
mod update;

pub use add::*;
pub use list::*;  // Add this
pub use disable::*;
pub use enable::*;
pub use update::*;
```

4. **Register route** — Update [src/startup.rs](src/startup.rs) in the `/client` scope:
```rust
.service(
    web::scope("/client")
        .service(routes::client::list_handler)  // Add this
        .service(routes::client::add_handler)
        .service(routes::client::update_handler)
        .service(routes::client::enable_handler)
        .service(routes::client::disable_handler),
)
```

5. **Add Casbin rule** — Create migration `migrations/20240101000000_client_list_rule.up.sql`:
```sql
INSERT INTO public.casbin_rule (ptype, v0, v1, v2) 
VALUES ('p', 'group_user', '/client', 'GET');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2) 
VALUES ('p', 'group_admin', '/client', 'GET');
```

6. **Test** — Run `make test TESTS=routes::client` to verify

**Full checklist:**
- [ ] Handler created with `#[tracing::instrument]` macro
- [ ] Database query added with SQLx macros
- [ ] Handler exported in mod.rs
- [ ] Route registered in startup.rs
- [ ] Casbin rules added for all affected groups (admin/user/anonym)
- [ ] Tests pass: `make test`
- [ ] Lint passes: `make lint`

### Testing Pattern
- Tests co-located with code (see `#[cfg(test)]` in source files)
- Mock data in [tests/mock_data/](tests/mock_data) (YAML fixtures)
- Single-threaded to ensure database state isolation

## Integration Points & External Services

### RabbitMQ (AMQP)
- **Purpose**: Deployment status updates from TryDirect Install service
- **Connection**: [MqManager](src/helpers) in startup, injected as `web::Data`
- **Queue connection string**: `amqp://username:password@host:port/%2f`
- **Config**: [configuration.yaml.dist](configuration.yaml.dist) has `amqp` section

### TryDirect External API
- **OAuth endpoint**: `auth_url` from configuration
- **Deploy service**: Receives `/project/deploy` requests, sends status via RabbitMQ

### Docker Compose Generation
Route: [src/routes/project/compose.rs](src/routes/project/compose.rs)
Validates & generates Docker Compose YAML from project JSON.

## Project-Specific Conventions

### Tracing & Observability
All routes have `#[tracing::instrument(name = "...")]` macro for structured logging:
```rust
#[tracing::instrument(name = "Get project list.")]
```
Configured with Bunyan formatter for JSON output.

### Error Handling
No exception-based unwinding—use `Result<T, E>` with `map_err` chains. Convert errors to `JsonResponse::internal_server_error()` or appropriate HTTP status.

### Configuration Management
- Load from `configuration.yaml` at startup (see [src/configuration.rs](src/configuration.rs))
- Available in routes via `web::Data<Settings>`
- Never hardcode secrets; use environment config

## Debugging Authentication & Authorization

### 403 Forbidden Errors
When an endpoint returns 403, work through this checklist in order:

1. **Check Casbin rule exists**
   - Query DB: `SELECT * FROM casbin_rule WHERE v1 = '/endpoint_path' AND v2 = 'METHOD'`
   - Verify subject (`v0`) includes your role or a group your role inherits from
   - Example: User with role `user_alice` needs rule with v0 = `user_alice`, `group_user`, or `group_anonymous`

2. **Verify path pattern matches**
   - Casbin uses `keyMatch2()` for path patterns (e.g., `/client/:id` matches `/client/123`)
   - Pattern `/client` does NOT match `/client/:id`—need separate rules for each path

3. **Check role assignment**
   - Verify user's role from auth service matches an existing role in DB
   - Test: Add rule for `p, any_test_subject, /endpoint_path, GET` temporarily
   - If 403 persists, issue is in authentication (step 2 failed), not authorization

4. **View logs**
   - Tracing logs show: `ACL check for role: {role}` when OAuth succeeds
   - Look for `"subject": "anonym"` if expecting authenticated request
   - HMAC failures log: `client is not active` (secret is NULL) or hash mismatch

### Testing Authentication
Tests co-located in source files. Example from [src/routes/client/add.rs](src/routes/client/add.rs):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};
    use sqlx::postgres::PostgresPool;

    #[actix_web::test]
    async fn test_add_client_authenticated() {
        let pool = setup_test_db().await; // From test fixtures
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .route("/client", web::post().to(add_handler))
        )
        .await;

        // Simulate OAuth user (injected via middleware in real flow)
        let req = test::TestRequest::post()
            .uri("/client")
            .insert_header(("Authorization", "Bearer test_token"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }
}
```

### Testing HMAC Signature
When testing HMAC endpoints, compute signature correctly:

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

let body = r#"{"name":"test"}"#;
let secret = "client_secret_from_db";
let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
mac.update(body.as_bytes());
let hash = format!("{:x}", mac.finalize().into_bytes());

let req = test::TestRequest::post()
    .uri("/client")
    .insert_header(("stacker-id", "123"))
    .insert_header(("stacker-hash", hash))
    .set_payload(body)
    .to_request();
```

### Adding a New Role Group
To create a new role hierarchy (e.g., `group_service` for internal microservices):

1. **Migration**: Add inheritance rules
```sql
-- Create role group
INSERT INTO public.casbin_rule (ptype, v0, v1) 
VALUES ('g', 'group_service', 'group_anonymous');

-- Assign specific service to group
INSERT INTO public.casbin_rule (ptype, v0, v1) 
VALUES ('g', 'service_deploy', 'group_service');

-- Grant permissions to group
INSERT INTO public.casbin_rule (ptype, v0, v1, v2) 
VALUES ('p', 'group_service', '/project/:id/deploy', 'POST');
```

2. **OAuth integration**: Service must authenticate with a Bearer token containing role `service_deploy`
3. **Verify inheritance**: Test that `service_deploy` inherits all `group_service` and `group_anonymous` permissions

## Test Quality Standard ⭐ **CRITICAL**

**ONLY write real, meaningful tests. NEVER write garbage tests or trivial assertions.**

### What Constitutes a Real Test

✅ **Good Tests**:
- Test actual handler/route behavior (HTTP request → response)
- Use real database interactions (or meaningful mocks that verify behavior)
- Test error cases with realistic scenarios
- Verify business logic, not trivial comparisons
- Integration tests that prove the feature works end-to-end
- Tests that would fail if the feature broke

❌ **Garbage Tests to AVOID**:
- Unit tests that just assert `assert_eq!("a", "a")`
- Tests that mock everything away so nothing is actually tested
- One-liner tests like `assert!(None.is_none())`
- Tests that don't test the real code path (just testing helpers/utilities)
- Tests that would pass even if the feature is completely broken
- Tests that test trivial string comparisons or variable assignments

### Examples

**BAD** (Garbage - Don't write this):
```rust
#[test]
fn test_plan_hierarchy() {
    let user_plan = "enterprise";
    let required_plan = "professional";
    assert_ne!(user_plan, required_plan);  // ← Just comparing strings, tests nothing real
}
```

**GOOD** (Real - Write this):
```rust
#[actix_web::test]
async fn test_deployment_blocked_for_insufficient_plan() {
    // Setup: Create actual project + template with plan requirement in DB
    // Execute: Call deploy handler with user lacking required plan
    // Assert: Returns 403 Forbidden with correct error message
}
```

### When to Skip Tests

If proper integration testing requires:
- Database setup that's complex
- External service mocks that would be fragile
- Test infrastructure that doesn't exist yet

**BETTER to have no test than a garbage test.** Document the missing test in code comments, not with fake tests that pass meaninglessly.

### Rule of Thumb

Ask: **"Would this test fail if someone completely removed/broke the feature?"**

If answer is "no" → It's a garbage test, don't write it.

---

## Common Gotchas & Quick Reference

| Issue | Fix |
|-------|-----|
| New endpoint returns 403 Forbidden | Check Casbin rule exists + path pattern matches + user role inherits from rule subject |
| HMAC signature fails in tests | Ensure body is exact same bytes (no formatting changes) and secret matches DB |
| OAuth token rejected | Bearer token missing "Bearer " prefix, or auth_url in config is wrong |
| SQLx offline compilation fails | Run `cargo sqlx prepare` after changing DB queries |
| Database changes not applied | Run `docker-compose down && docker-compose up` then `sqlx migrate run` |
| User data access denied in handler | Verify `user: web::ReqData<Arc<models::User>>` injected and Casbin subject matches |
| Casbin rule works in migration but 403 persists | Migration not applied—restart with `sqlx migrate run` |

## Key Files for Reference
- Startup/config: [src/main.rs](src/main.rs), [src/startup.rs](src/startup.rs)
- Middleware: [src/middleware/](src/middleware)
- Route examples: [src/routes/project/get.rs](src/routes/project/get.rs)
- Database queries: [src/db/project.rs](src/db/project.rs)
- Migrations: [migrations/](migrations)
