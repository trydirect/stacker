# Implementation Roadmap - Open Questions Resolutions

**Generated**: 9 January 2026  
**Based On**: [OPEN_QUESTIONS_RESOLUTIONS.md](OPEN_QUESTIONS_RESOLUTIONS.md)  
**Status**: Ready for sprint planning

---

## Implementation Tasks

### Phase 1: Stacker Health Check Endpoint (Priority 1)

**Task 1.1**: Create health check route
- **File**: `src/routes/health.rs` (new)
- **Endpoint**: `GET /api/health/deployment/{deployment_hash}/app/{app_code}`
- **Scope**:
  - Verify deployment exists in database
  - Get app configuration from `deployment` and `project` tables
  - Execute health probe (HTTP GET to app's health URL)
  - Aggregate status and return JSON response
  - Handle timeouts gracefully (10s default)
- **Tests**: Unit tests for health probe logic, integration test with real deployment
- **Estimate**: 2-3 hours
- **Owner**: TBD

**Task 1.2**: Add Casbin authorization rules
- **File**: `migrations/20260109000000_health_check_casbin_rules.up.sql` (new)
- **Scope**:
  - Add rules for `group_anonymous` and `group_user` to GET health check endpoint
  - Pattern: `/api/health/deployment/:deployment_hash/app/:app_code`
- **Estimate**: 30 minutes
- **Owner**: TBD

**Task 1.3**: Configuration for health check timeout
- **File**: `configuration.yaml` and `src/configuration.rs`
- **Scope**:
  - Add `health_check.timeout_secs` setting (default: 10)
  - Add `health_check.interval_secs` (default: 30)
  - Load in startup
- **Estimate**: 30 minutes
- **Owner**: TBD

**Task 1.4**: Integration with Status Panel contract
- **File**: Documentation update
- **Scope**:
  - Document expected behavior in [MCP_SERVER_BACKEND_PLAN.md](MCP_SERVER_BACKEND_PLAN.md)
  - Define health check response format
- **Estimate**: 1 hour
- **Owner**: TBD

---

### Phase 2: Rate Limiter Middleware (Priority 1)

**Task 2.1**: Create rate limiter service
- **File**: `src/middleware/rate_limiter.rs` (new)
- **Scope**:
  - Create Redis-backed rate limit checker
  - Support per-user rate limiting
  - Support configurable limits per endpoint
  - Return 429 Too Many Requests with Retry-After header
- **Tests**: Unit tests with mock Redis, integration tests
- **Estimate**: 3-4 hours
- **Owner**: TBD

**Task 2.2**: Configure rate limits
- **File**: `configuration.yaml`
- **Scope**:
  ```yaml
  rate_limits:
    deploy: { per_minute: 10, per_hour: 100 }
    restart: { per_minute: 5, per_hour: 50 }
    status_check: { per_minute: 60 }
    logs: { per_minute: 20, per_hour: 200 }
  ```
- **Estimate**: 30 minutes
- **Owner**: TBD

**Task 2.3**: Apply rate limiter to endpoints
- **Files**: 
  - `src/routes/project/deploy.rs`
  - `src/routes/deployment/restart.rs`
  - `src/routes/deployment/logs.rs`
  - `src/routes/deployment/status.rs`
- **Scope**:
  - Apply `#[rate_limit("deploy")]` macro to deploy endpoints
  - Apply `#[rate_limit("restart")]` to restart endpoints
  - Apply `#[rate_limit("logs")]` to log endpoints
  - Add integration tests
- **Estimate**: 2 hours
- **Owner**: TBD

**Task 2.4**: Expose rate limits to User Service
- **File**: `src/routes/user/rate_limits.rs` (new)
- **Endpoint**: `GET /api/user/rate-limits`
- **Response**: JSON with current limits per endpoint
- **Scope**:
  - Load from config
  - Return to User Service for plan-based enforcement
- **Estimate**: 1 hour
- **Owner**: TBD

---

### Phase 3: Log Redaction Service (Priority 2)

**Task 3.1**: Create log redactor service
- **File**: `src/services/log_redactor.rs` (new)
- **Scope**:
  - Define 6 pattern categories (env vars, cloud creds, API tokens, PII, credit cards, SSH keys)
  - Define 20 env var names blacklist
  - Implement `redact_logs(input: &str) -> String`
  - Implement `redact_env_vars(vars: HashMap) -> HashMap`
- **Tests**: Unit tests for each pattern, integration test with real deployment logs
- **Estimate**: 3 hours
- **Owner**: TBD

**Task 3.2**: Apply redaction to log endpoints
- **File**: `src/routes/deployment/logs.rs`
- **Scope**:
  - Call `log_redactor::redact_logs()` before returning
  - Add `"redacted": true` flag to response
  - Document which rules were applied
- **Estimate**: 1 hour
- **Owner**: TBD

**Task 3.3**: Document redaction policy
- **File**: `docs/SECURITY_LOG_REDACTION.md` (new)
- **Scope**:
  - List all redaction patterns
  - Explain why each is redacted
  - Show before/after examples
- **Estimate**: 1 hour
- **Owner**: TBD

---

### Phase 4: User Service Schema Changes (Priority 1)

**Task 4.1**: Create `deployment_apps` table
- **File**: `migrations_for_trydirect/20260109000000_create_deployment_apps.up.sql` (new)
- **Scope**:
  ```sql
  CREATE TABLE deployment_apps (
      id UUID PRIMARY KEY,
      deployment_hash VARCHAR(64),
      installation_id INTEGER,
      app_code VARCHAR(255),
      container_name VARCHAR(255),
      image VARCHAR(255),
      ports JSONB,
      metadata JSONB,
      created_at TIMESTAMP,
      updated_at TIMESTAMP,
      FOREIGN KEY (installation_id) REFERENCES installations(id)
  );
  CREATE INDEX idx_deployment_hash ON deployment_apps(deployment_hash);
  CREATE INDEX idx_app_code ON deployment_apps(app_code);
  ```
- **Estimate**: 1 hour
- **Owner**: User Service team

**Task 4.2**: Create User Service endpoint
- **File**: `app/api/routes/deployments.py` (User Service)
- **Endpoint**: `GET /api/1.0/deployments/{deployment_hash}/apps`
- **Scope**:
  - Query `deployment_apps` table
  - Return app list with code, container name, image, ports
- **Estimate**: 1 hour
- **Owner**: User Service team

**Task 4.3**: Update deployment creation logic
- **File**: `app/services/deployment_service.py` (User Service)
- **Scope**:
  - When creating deployment, populate `deployment_apps` from project metadata
  - Extract app_code, container_name, image, ports
- **Estimate**: 2 hours
- **Owner**: User Service team

---

### Phase 5: Integration & Testing (Priority 2)

**Task 5.1**: End-to-end health check test
- **File**: `tests/integration/health_check.rs` (Stacker)
- **Scope**:
  - Deploy a test stack
  - Query health check endpoint
  - Verify response format and status codes
- **Estimate**: 2 hours
- **Owner**: TBD

**Task 5.2**: Rate limiter integration test
- **File**: `tests/integration/rate_limiter.rs` (Stacker)
- **Scope**:
  - Test rate limit exceeded scenario
  - Verify 429 response and Retry-After header
  - Test reset after timeout
- **Estimate**: 1.5 hours
- **Owner**: TBD

**Task 5.3**: Log redaction integration test
- **File**: `tests/integration/log_redaction.rs` (Stacker)
- **Scope**:
  - Create deployment with sensitive env vars
  - Retrieve logs
  - Verify sensitive data is redacted
- **Estimate**: 1.5 hours
- **Owner**: TBD

**Task 5.4**: Status Panel integration test
- **File**: `tests/integration/status_panel_integration.rs`
- **Scope**:
  - Status Panel queries health checks for deployed apps
  - Verify Status Panel can use app_code from deployment_apps
- **Estimate**: 2 hours
- **Owner**: Status Panel team

---

### Phase 6: Documentation & Deployment (Priority 3)

**Task 6.1**: Update API documentation
- **Files**: 
  - `docs/USER_SERVICE_API.md` (health check, rate limits)
  - `docs/STACKER_API.md` (new or updated)
  - `docs/MCP_SERVER_BACKEND_PLAN.md`
- **Scope**:
  - Document new endpoints with curl examples
  - Document rate limit headers
  - Document redaction behavior
- **Estimate**: 2 hours
- **Owner**: TBD

**Task 6.2**: Update CHANGELOG
- **File**: `CHANGELOG.md`
- **Scope**:
  - Record all new features
  - Note breaking changes (if any)
  - Link to implementation tickets
- **Estimate**: 30 minutes
- **Owner**: TBD

**Task 6.3**: Monitoring & alerting
- **File**: Configuration updates
- **Scope**:
  - Add health check failure alerts
  - Add rate limit violation alerts
  - Monitor log redaction performance
- **Estimate**: 1-2 hours
- **Owner**: DevOps team

**Task 6.4**: Team communication
- **Scope**:
  - Present resolutions to team
  - Collect feedback and adjust
  - Finalize before implementation
- **Estimate**: 1 hour
- **Owner**: Project lead

---

## Summary by Phase

| Phase | Name | Tasks | Est. Hours | Priority |
|-------|------|-------|-----------|----------|
| 1 | Health Check | 4 | 6-7 | 1 |
| 2 | Rate Limiter | 4 | 6-7 | 1 |
| 3 | Log Redaction | 3 | 5 | 2 |
| 4 | User Service Schema | 3 | 3-4 | 1 |
| 5 | Integration Testing | 4 | 6-7 | 2 |
| 6 | Documentation | 4 | 4-5 | 3 |
| **Total** | | **22** | **30-35 hours** | — |

---

## Dependencies & Sequencing

```
Phase 1 (Health Check)    ──┐
Phase 2 (Rate Limiter)    ──┼──→ Phase 5 (Integration Testing)
Phase 3 (Log Redaction)   ──┤
Phase 4 (User Service)    ──┘
                              ↓
                        Phase 6 (Docs & Deploy)
```

**Critical Path**: Phase 1 & 4 must complete before Phase 5  
**Parallel Work**: Phases 1-4 can be worked on simultaneously with different teams

---

## Next Actions

1. **Review** [OPEN_QUESTIONS_RESOLUTIONS.md](OPEN_QUESTIONS_RESOLUTIONS.md)
2. **Confirm** all proposals with team
3. **Assign** tasks to engineers
4. **Update** sprint planning with implementation tasks
5. **Coordinate** with User Service and Status Panel teams

---

**Generated by**: Research task on 2026-01-09  
**Status**: Ready for team review and sprint planning
