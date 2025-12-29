# Stacker Development TODO

## MCP Tool Development

- [ ] **GenerateComposeTool Implementation**
  - Currently: Tool removed during Phase 3 due to ProjectForm schema complexity
  - Issue: Needs proper understanding of ProjectForm structure (especially `custom.web` array and nested docker_image fields)
  - TODO: 
    1. Inspect actual ProjectForm structure in [src/forms/project/](src/forms/project/)
    2. Map correct field paths for docker_image (namespace, repository, tag) and port configuration
    3. Implement Docker Compose YAML generation from project metadata
  - Reference: Previous implementation in [src/mcp/tools/compose.rs](src/mcp/tools/compose.rs)
  - Status: Phase 3 complete with 15 tools (9 Phase 3 tools without GenerateComposeTool)

- [ ] **MCP Browser-Based Client Support (Cookie Authentication)**
  - Currently: Backend supports Bearer token auth (works for server-side clients like wscat, CLI tools)
  - Issue: Browser WebSocket API cannot set `Authorization` header (W3C spec limitation)
  - Impact: Browser-based MCP UI clients cannot connect (get 403 Forbidden)
  - TODO:
    1. Create `src/middleware/authentication/method/f_cookie.rs` - Extract `access_token` from Cookie header
    2. Update `src/middleware/authentication/manager_middleware.rs` - Add `try_cookie()` after `try_oauth()`
    3. Export cookie method in `src/middleware/authentication/method/mod.rs`
    4. Test with wscat: `wscat -c ws://localhost:8000/mcp -H "Cookie: access_token=..."`
    5. Test with browser WebSocket connection
  - Reference: Full implementation guide in [docs/MCP_BROWSER_AUTH.md](docs/MCP_BROWSER_AUTH.md)
  - Priority: Medium (only needed for browser-based MCP clients)
  - Status: Server-side clients work perfectly; browser support blocked until cookie auth added
  - Note: Both auth methods should coexist - Bearer for servers, cookies for browsers

## Agent Registration & Security

- [ ] **Agent Registration Access Control**
  - Currently: `POST /api/v1/agent/register` is public (no auth required)
  - Issue: Any unauthenticated client can register agents
  - TODO: Require user authentication or API client credentials
  - Solution: Restore `user: web::ReqData<Arc<models::User>>` parameter in [src/routes/agent/register.rs](src/routes/agent/register.rs#L28) and add authorization check to verify user owns the deployment
  - Reference: See [src/routes/agent/register.rs](src/routes/agent/register.rs) line 28

- [ ] **Vault Client Testing**
  - Currently: Vault token storage fails gracefully in tests (falls back to bearer token when Vault unreachable at localhost)
  - TODO: Test against a real Vault instance
  - Steps:
    1. Spin up Vault in Docker or use a test environment
    2. Update [src/middleware/authentication/method/f_agent.rs](src/middleware/authentication/method/f_agent.rs) to use realistic Vault configuration
    3. Remove the localhost fallback once production behavior is validated
    4. Run integration tests with real Vault credentials

## OAuth & Authentication Improvements

- [ ] **OAuth Mock Server Lifecycle**
  - Issue: Mock auth server in tests logs "unable to connect" even though it's listening
  - Current fix: OAuth middleware has loopback fallback that synthesizes test users
  - TODO: Investigate why sanity check fails while actual requests succeed
  - File: [tests/common/mod.rs](tests/common/mod.rs#L45-L50)

- [ ] **Middleware Panic Prevention**
  - Current: Changed `try_lock().expect()` to return `Poll::Pending` to avoid panics during concurrent requests
  - TODO: Review this approach for correctness; consider if Mutex contention is expected
  - File: [src/middleware/authentication/manager_middleware.rs](src/middleware/authentication/manager_middleware.rs#L23-L27)

## Code Quality & Warnings

- [ ] **Deprecated Config Merge**
  - Warning: `config::Config::merge` is deprecated
  - File: [src/configuration.rs](src/configuration.rs#L70)
  - TODO: Use `ConfigBuilder` instead

- [ ] **Snake Case Violations**
  - Files with non-snake-case variable names:
    - [src/console/commands/debug/casbin.rs](src/console/commands/debug/casbin.rs#L31) - `authorizationService`
    - [src/console/commands/debug/dockerhub.rs](src/console/commands/debug/dockerhub.rs#L27) - `dockerImage`
    - [src/console/commands/debug/dockerhub.rs](src/console/commands/debug/dockerhub.rs#L29) - `isActive`
    - [src/helpers/dockerhub.rs](src/helpers/dockerhub.rs#L124) - `dockerHubToken`

- [ ] **Unused Fields & Functions**
  - [src/db/agreement.rs](src/db/agreement.rs#L30) - `fetch_by_user` unused
  - [src/db/agreement.rs](src/db/agreement.rs#L79) - `fetch_one_by_name` unused
  - [src/routes/agent/register.rs](src/routes/agent/register.rs#L9) - `public_key` field in RegisterAgentRequest never used
  - [src/routes/agent/report.rs](src/routes/agent/report.rs#L14) - `started_at` and `completed_at` fields in CommandReportRequest never read
  - [src/helpers/json.rs](src/helpers/json.rs#L100) - `no_content()` method never used
  - [src/models/rules.rs](src/models/rules.rs#L4) - `comments_per_user` field never read
  - [src/routes/test/deploy.rs](src/routes/test/deploy.rs#L8) - `DeployResponse` never constructed
  - [src/forms/rating/useredit.rs](src/forms/rating/useredit.rs#L18, L22) - `insert()` calls with unused return values
  - [src/forms/rating/adminedit.rs](src/forms/rating/adminedit.rs#L19, L23, L27) - `insert()` calls with unused return values
  - [src/forms/project/app.rs](src/forms/project/app.rs#L138) - Loop over Option instead of if-let

## Agent/Command Features

- [ ] **Long-Polling Timeout Handling**
  - Current: Wait endpoint holds connection for up to 30 seconds
  - TODO: Document timeout behavior in API docs
  - File: [src/routes/agent/wait.rs](src/routes/agent/wait.rs)

- [ ] **Command Priority Ordering**
  - Current: Commands returned in priority order (critical > high > normal > low)
  - TODO: Add tests for priority edge cases and fairness among same-priority commands

- [ ] **Agent Heartbeat & Status**
  - Current: Agent status tracked in `agents.status` and `agents.last_heartbeat`
  - TODO: Implement agent timeout detection (e.g., mark offline if no heartbeat > 5 minutes)
  - TODO: Add health check endpoint for deployment dashboards

## Deployment & Testing

- [ ] **Full Test Suite**
  - Current: Agent command flow tests pass (4/5 passing, 1 ignored)
  - TODO: Run full `cargo test` suite and fix any remaining failures
  - TODO: Add tests for project body→metadata migration edge cases

- [ ] **Database Migration Safety**
  - Current: Duplicate Casbin migration neutralized (20251223100000_casbin_agent_rules.up.sql is a no-op)
  - TODO: Clean up or document why this file exists
  - TODO: Add migration validation in CI/CD

## Documentation

- [ ] **API Documentation**
  - TODO: Add OpenAPI/Swagger definitions for agent endpoints
  - TODO: Document rate limiting policies for API clients

- [ ] **Agent Developer Guide**
  - TODO: Create quickstart for agent implementers
  - TODO: Provide SDKs or client libraries for agent communication

## Performance & Scalability

- [ ] **Long-Polling Optimization**
  - Current: Simple 30-second timeout poll
  - TODO: Consider Server-Sent Events (SSE) or WebSocket for real-time command delivery
  - TODO: Add metrics for long-poll latency and agent responsiveness

- [ ] **Database Connection Pooling**
  - TODO: Review SQLx pool configuration for production load
  - TODO: Add connection pool metrics

## Security

- [ ] **Agent Token Rotation**
  - TODO: Implement agent token expiration
  - TODO: Add token refresh mechanism

- [ ] **Casbin Rule Validation**
  - Current: Casbin rules require manual maintenance
  - TODO: Add schema validation for Casbin rules at startup
  - TODO: Add lint/check command to validate rules

## Known Issues

- [ ] **SQLx Offline Mode**
  - Current: Using `sqlx` in offline mode; some queries may not compile if schema changes
  - TODO: Document how to regenerate `.sqlx` cache: `cargo sqlx prepare`

- [ ] **Vault Fallback in Tests**
  - Current: [src/middleware/authentication/method/f_agent.rs](src/middleware/authentication/method/f_agent.rs#L90-L103) has loopback fallback
  - Risk: Could mask real Vault errors in non-test environments
  - TODO: Add feature flag or config to control fallback behavior
