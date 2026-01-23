# Open Questions Resolution - Status Panel & MCP Integration

**Date**: 9 January 2026  
**Status**: Proposed Answers (Awaiting Team Confirmation)  
**Related**: [TODO.md - New Open Questions](../TODO.md#new-open-questions-status-panel--mcp)

---

## Question 1: Health Check Contract Per App

**Original Question**: What is the exact URL/expected status/timeout that Status Panel should register and return?

### Context
- Status Panel (part of User Service) needs to monitor deployed applications' health
- Stacker has already created health check endpoint infrastructure:
  - Migration: `20260103120000_casbin_health_metrics_rules.up.sql` (Casbin rules for `/health_check/metrics`)
  - Endpoint: `/health_check` (registered via Casbin rules for `group_anonymous`)
- Each deployed app container needs its own health check URL

### Proposed Contract

**Health Check Endpoint Pattern**:
```
GET /api/health/deployment/{deployment_hash}/app/{app_code}
```

**Response Format** (JSON):
```json
{
  "status": "healthy|degraded|unhealthy",
  "timestamp": "2026-01-09T12:00:00Z",
  "deployment_hash": "abc123...",
  "app_code": "nginx",
  "details": {
    "response_time_ms": 42,
    "checks": [
      {"name": "database_connection", "status": "ok"},
      {"name": "disk_space", "status": "ok", "used_percent": 65}
    ]
  }
}
```

**Status Codes**:
- `200 OK` - All checks passed (healthy)
- `202 Accepted` - Partial degradation (degraded)
- `503 Service Unavailable` - Critical failure (unhealthy)

**Default Timeout**: 10 seconds per health check
- Configurable via `configuration.yaml`: `health_check.timeout_secs`
- Status Panel should respect `Retry-After` header if `503` returned

### Implementation in Stacker

**Route Handler Location**: `src/routes/health.rs`
```rust
#[get("/api/health/deployment/{deployment_hash}/app/{app_code}")]
pub async fn app_health_handler(
    path: web::Path<(String, String)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (deployment_hash, app_code) = path.into_inner();
    
    // 1. Verify deployment exists
    // 2. Get app configuration from deployment_apps table
    // 3. Execute health check probe (HTTP GET to container port)
    // 4. Aggregate results
    // 5. Return JsonResponse with status
}
```

**Casbin Rule** (to be added):
```sql
INSERT INTO public.casbin_rule (ptype, v0, v1, v2) 
VALUES ('p', 'group_anonymous', '/api/health/deployment/:deployment_hash/app/:app_code', 'GET');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2) 
VALUES ('p', 'group_user', '/api/health/deployment/:deployment_hash/app/:app_code', 'GET');
```

**Status Panel Registration** (User Service):
```python
# Register health check with Status Panel service
health_checks = [
    {
        "name": f"{app_code}",
        "url": f"https://stacker-api/api/health/deployment/{deployment_hash}/app/{app_code}",
        "timeout_secs": 10,
        "interval_secs": 30,  # Check every 30 seconds
        "expected_status": 200,  # Accept 200 or 202
        "expected_body_contains": '"status"'
    }
    for app_code in deployment_apps
]
```

---

## Question 2: Per-App Deploy Trigger Rate Limits

**Original Question**: What are the allowed requests per minute/hour to expose in User Service?

### Context
- Deploy endpoints are at risk of abuse (expensive cloud operations)
- Need consistent rate limiting across services
- User Service payment system needs to enforce limits per plan tier

### Proposed Rate Limits

**By Endpoint Type**:

| Endpoint | Limit | Window | Applies To |
|----------|-------|--------|-----------|
| `POST /project/:id/deploy` | 10 req/min | Per minute | Single deployment |
| `GET /deployment/:hash/status` | 60 req/min | Per minute | Status polling |
| `POST /deployment/:hash/restart` | 5 req/min | Per minute | Restart action |
| `POST /deployment/:hash/logs` | 20 req/min | Per minute | Log retrieval |
| `POST /project/:id/compose/validate` | 30 req/min | Per minute | Validation (free) |

**By Plan Tier** (negotiable):

| Plan | Deploy/Hour | Restart/Hour | Concurrent |
|------|-------------|--------------|-----------|
| Free | 5 | 3 | 1 |
| Plus | 20 | 10 | 3 |
| Enterprise | 100 | 50 | 10 |

### Implementation in Stacker

**Rate Limit Configuration** (`configuration.yaml`):
```yaml
rate_limits:
  deploy:
    per_minute: 10
    per_hour: 100
    burst_size: 2  # Allow 2 burst requests
  restart:
    per_minute: 5
    per_hour: 50
  status_check:
    per_minute: 60
    per_hour: 3600
  logs:
    per_minute: 20
    per_hour: 200
```

**Rate Limiter Middleware** (Redis-backed):
```rust
// src/middleware/rate_limiter.rs
pub async fn rate_limit_middleware(
    req: ServiceRequest,
    srv: S,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let redis_client = req.app_data::<web::Data<RedisClient>>()?;
    let user_id = req.extensions().get::<Arc<User>>()?.id.clone();
    let endpoint = req.path();
    
    let key = format!("rate_limit:{}:{}", user_id, endpoint);
    let count = redis_client.incr(&key).await?;
    
    if count > LIMIT {
        return Err(actix_web::error::error_handler(
            actix_web::error::ErrorTooManyRequests("Rate limit exceeded")
        ));
    }
    
    redis_client.expire(&key, 60).await?;  // 1-minute window
    
    srv.call(req).await?.map_into_right_body()
}
```

**User Service Contract** (expose limits):
```python
# GET /api/1.0/user/rate-limits
{
    "deploy": {"per_minute": 20, "per_hour": 200},
    "restart": {"per_minute": 10, "per_hour": 100},
    "status_check": {"per_minute": 60},
    "logs": {"per_minute": 20, "per_hour": 200}
}
```

---

## Question 3: Log Redaction Patterns

**Original Question**: Which env var names/secret regexes should be stripped before returning logs via Stacker/User Service?

### Context
- Logs often contain environment variables and secrets
- Must prevent accidental exposure of AWS keys, API tokens, passwords
- Pattern must be consistent across Stacker → User Service → Status Panel

### Proposed Redaction Patterns

**Redaction Rules** (in priority order):

```yaml
redaction_patterns:
  # 1. Environment Variables (most sensitive)
  - pattern: '(?i)(API_KEY|SECRET|PASSWORD|TOKEN|CREDENTIAL)\s*=\s*[^\s]+'
    replacement: '$1=***REDACTED***'
    
  # 2. AWS & Cloud Credentials
  - pattern: '(?i)(AKIAIOSFODNN7EXAMPLE|aws_secret_access_key|AWS_SECRET)\s*=\s*[^\s]+'
    replacement: '***REDACTED***'
  
  - pattern: '(?i)(database_url|db_password|mysql_root_password|PGPASSWORD)\s*=\s*[^\s]+'
    replacement: '$1=***REDACTED***'
    
  # 3. API Keys & Tokens
  - pattern: '(?i)(authorization|auth_token|bearer)\s+[A-Za-z0-9._\-]+'
    replacement: '$1 ***TOKEN***'
    
  - pattern: 'Basic\s+[A-Za-z0-9+/]+={0,2}'
    replacement: 'Basic ***CREDENTIALS***'
    
  # 4. Email & PII (lower priority)
  - pattern: '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}'
    replacement: '***EMAIL***'
    
  # 5. Credit Card Numbers
  - pattern: '\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b'
    replacement: '****-****-****-****'
    
  # 6. SSH Keys
  - pattern: '-----BEGIN.*PRIVATE KEY-----[\s\S]*?-----END.*PRIVATE KEY-----'
    replacement: '***PRIVATE KEY REDACTED***'
```

**Environment Variable Names to Always Redact**:
```rust
const REDACTED_ENV_VARS: &[&str] = &[
    // AWS
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    // Database
    "DATABASE_URL",
    "DB_PASSWORD",
    "MYSQL_ROOT_PASSWORD",
    "PGPASSWORD",
    "MONGO_PASSWORD",
    // API Keys
    "API_KEY",
    "API_SECRET",
    "AUTH_TOKEN",
    "SECRET_KEY",
    "PRIVATE_KEY",
    // Third-party services
    "STRIPE_SECRET_KEY",
    "STRIPE_API_KEY",
    "TWILIO_AUTH_TOKEN",
    "GITHUB_TOKEN",
    "GITLAB_TOKEN",
    "SENDGRID_API_KEY",
    "MAILGUN_API_KEY",
    // TLS/SSL
    "CERT_PASSWORD",
    "KEY_PASSWORD",
    "SSL_KEY_PASSWORD",
];
```

### Implementation in Stacker

**Log Redactor Service** (`src/services/log_redactor.rs`):
```rust
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref REDACTION_RULES: Vec<(Regex, &'static str)> = vec![
        (Regex::new(r"(?i)(API_KEY|SECRET|PASSWORD|TOKEN)\s*=\s*[^\s]+").unwrap(),
         "$1=***REDACTED***"),
        // ... more patterns
    ];
}

pub fn redact_logs(input: &str) -> String {
    let mut output = input.to_string();
    for (pattern, replacement) in REDACTION_RULES.iter() {
        output = pattern.replace_all(&output, *replacement).to_string();
    }
    output
}

pub fn redact_env_vars(vars: &HashMap<String, String>) -> HashMap<String, String> {
    vars.iter()
        .map(|(k, v)| {
            if REDACTED_ENV_VARS.contains(&k.as_str()) {
                (k.clone(), "***REDACTED***".to_string())
            } else {
                (k.clone(), v.clone())
            }
        })
        .collect()
}
```

**Applied in Logs Endpoint** (`src/routes/logs.rs`):
```rust
#[get("/api/deployment/{deployment_hash}/logs")]
pub async fn get_logs_handler(
    path: web::Path<String>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let deployment_hash = path.into_inner();
    
    // Fetch raw logs from database
    let raw_logs = db::deployment::fetch_logs(pg_pool.get_ref(), &deployment_hash)
        .await
        .map_err(|e| JsonResponse::build().internal_server_error(e))?;
    
    // Redact sensitive information
    let redacted_logs = log_redactor::redact_logs(&raw_logs);
    
    Ok(JsonResponse::build()
        .set_item(Some(json!({"logs": redacted_logs})))
        .ok("OK"))
}
```

**User Service Contract** (expose redaction status):
```python
# GET /api/1.0/logs/{deployment_hash}
{
    "logs": "[2026-01-09T12:00:00Z] Starting app...",
    "redacted": True,
    "redaction_rules_applied": [
        "aws_credentials",
        "database_passwords",
        "api_tokens",
        "private_keys"
    ]
}
```

---

## Question 4: Container→App_Code Mapping

**Original Question**: Confirm canonical source (deployment_apps.metadata.container_name) for Status Panel health/logs responses?

### Context
- Stacker: Project metadata contains app definitions (app_code, container_name, ports)
- User Service: Deployments table (installations) tracks deployed instances
- Status Panel: Needs to map containers back to logical app codes for UI
- Missing: User Service doesn't have `deployment_apps` table yet—need to confirm schema

### Analysis of Current Structure

**Stacker Side** (from project metadata):
```rust
// Project.metadata structure:
{
  "apps": [
    {
      "app_code": "nginx",
      "container_name": "my-app-nginx",
      "image": "nginx:latest",
      "ports": [80, 443]
    },
    {
      "app_code": "postgres",
      "container_name": "my-app-postgres",
      "image": "postgres:15",
      "ports": [5432]
    }
  ]
}
```

**User Service Side** (TryDirect schema):
```sql
CREATE TABLE installations (
    _id INTEGER PRIMARY KEY,
    user_id INTEGER,
    stack_id INTEGER,        -- Links to Stacker project
    status VARCHAR(32),
    request_dump VARCHAR,    -- Contains app definitions
    token VARCHAR(100),
    _created TIMESTAMP,
    _updated TIMESTAMP
);
```

### Problem
- User Service `installations.request_dump` is opaque text (not structured schema)
- Status Panel cannot query app_code/container mappings from User Service directly
- Need a dedicated `deployment_apps` table for fast lookups

### Proposed Solution

**Create deployment_apps Table** (User Service):
```sql
CREATE TABLE deployment_apps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    deployment_hash VARCHAR(64) NOT NULL,  -- Links to Stacker.deployment
    installation_id INTEGER NOT NULL REFERENCES installations(id),
    app_code VARCHAR(255) NOT NULL,        -- Canonical source: from project metadata
    container_name VARCHAR(255) NOT NULL,   -- Docker container name
    image VARCHAR(255),
    ports JSONB,                           -- [80, 443]
    metadata JSONB,                        -- Flexible for Status Panel needs
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    
    FOREIGN KEY (installation_id) REFERENCES installations(id) ON DELETE CASCADE,
    INDEX idx_deployment_hash (deployment_hash),
    INDEX idx_app_code (app_code),
    UNIQUE (deployment_hash, app_code)
);
```

**Data Flow**:
1. **Stacker deploys** → Calls User Service `POST /install/init/` with project metadata
2. **User Service receives** → Extracts app definitions from project.metadata.apps
3. **User Service inserts** → Creates `deployment_apps` rows (one per app)
4. **Status Panel queries** → `GET /api/1.0/deployment/{deployment_hash}/apps`
5. **Status Panel uses** → `container_name` + `app_code` for health checks and logs

**Contract Between Stacker & User Service**:

Stacker sends deployment info:
```json
{
  "deployment_hash": "abc123...",
  "stack_id": 5,
  "apps": [
    {
      "app_code": "nginx",
      "container_name": "myapp-nginx",
      "image": "nginx:latest",
      "ports": [80, 443]
    }
  ]
}
```

User Service stores and exposes:
```python
# GET /api/1.0/deployments/{deployment_hash}/apps
{
  "deployment_hash": "abc123...",
  "apps": [
    {
      "id": "uuid-1",
      "app_code": "nginx",
      "container_name": "myapp-nginx",
      "image": "nginx:latest",
      "ports": [80, 443],
      "metadata": {}
    }
  ]
}
```

### Canonical Source Confirmation

**Answer: `app_code` is the canonical source.**

- **Origin**: Stacker `project.metadata.apps[].app_code`
- **Storage**: User Service `deployment_apps.app_code`
- **Reference**: Status Panel uses `app_code` as logical identifier for UI
- **Container Mapping**: `app_code` → `container_name` (1:1 mapping per deployment)

---

## Summary Table

| Question | Proposed Answer | Implementation |
|----------|-----------------|-----------------|
| **Health Check Contract** | `GET /api/health/deployment/{hash}/app/{code}` | New route in Stacker |
| **Rate Limits** | Deploy: 10/min, Restart: 5/min, Logs: 20/min | Middleware + config |
| **Log Redaction** | 6 pattern categories + 20 env var names | Service in Stacker |
| **Container Mapping** | `app_code` is canonical; use User Service `deployment_apps` table | Schema change in User Service |

---

## Next Steps

**Priority 1** (This Week):
- [ ] Confirm health check contract with team
- [ ] Confirm rate limit tiers with Product
- [ ] Create `deployment_apps` table migration in User Service

**Priority 2** (Next Week):
- [ ] Implement health check endpoint in Stacker
- [ ] Add log redaction service to Stacker
- [ ] Update User Service deployment creation to populate `deployment_apps`
- [ ] Update Status Panel to use new health check contract

**Priority 3**:
- [ ] Document final decisions in README
- [ ] Add integration tests
- [ ] Update monitoring/alerting for health checks

---

## Contact & Questions

For questions or changes to these proposals:
1. Update this document
2. Log in CHANGELOG.md
3. Notify team via shared memory tool (`/memories/open_questions.md`)
