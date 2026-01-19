# Admin Service & JWT Authentication Testing Plan

## Phase 1: Build & Deployment (Current)

**Goal:** Verify code compiles and container starts successfully

- [ ] Run `cargo check --lib` → no errors
- [ ] Build Docker image → successfully tagged
- [ ] Container starts → `docker compose up -d`
- [ ] Check logs → no panic/connection errors
  ```bash
  docker compose logs -f stacker | grep -E "error|panic|ACL check for JWT"
  ```

---

## Phase 2: Integration Testing (Admin Service JWT)

**Goal:** Verify JWT authentication and admin endpoints work

### 2.1 Generate Test JWT Token

```bash
# Generate a test JWT with admin_service role
python3 << 'EOF'
import json
import base64
import time

header = {"alg": "HS256", "typ": "JWT"}
exp = int(time.time()) + 3600  # 1 hour from now
payload = {"role": "admin_service", "email": "info@optimum-web.com", "exp": exp}

header_b64 = base64.urlsafe_b64encode(json.dumps(header).encode()).decode().rstrip('=')
payload_b64 = base64.urlsafe_b64encode(json.dumps(payload).encode()).decode().rstrip('=')
signature = "fake_signature"  # JWT parsing doesn't verify signature (internal service only)

token = f"{header_b64}.{payload_b64}.{signature}"
print(f"JWT_TOKEN={token}")
EOF
```

### 2.2 Test Admin Templates Endpoint

```bash
JWT_TOKEN="<paste from above>"

# Test 1: List submitted templates
curl -v \
  -H "Authorization: Bearer $JWT_TOKEN" \
  http://localhost:8000/stacker/admin/templates?status=pending

# Expected: 200 OK with JSON array of templates
# Check logs for: "JWT authentication successful for role: admin_service"
```

### 2.3 Verify Casbin Rules Applied

```bash
# Check database for admin_service rules
docker exec stackerdb psql -U postgres -d stacker -c \
  "SELECT * FROM casbin_rule WHERE v0='admin_service' AND v1 LIKE '%admin%';"

# Expected: 6 rows (GET/POST on /admin/templates, /:id/approve, /:id/reject for both /stacker and /api prefixes)
```

### 2.4 Test Error Cases

```bash
# Test 2: No token (should fall back to OAuth, get 401)
curl -v http://localhost:8000/stacker/admin/templates

# Test 3: Invalid token format
curl -v \
  -H "Authorization: InvalidScheme $JWT_TOKEN" \
  http://localhost:8000/stacker/admin/templates

# Test 4: Expired token
PAST_EXP=$(python3 -c "import time; print(int(time.time()) - 3600)")
# Generate JWT with exp=$PAST_EXP, should get 401 "JWT token expired"

# Test 5: Malformed JWT (not 3 parts)
curl -v \
  -H "Authorization: Bearer not.a.jwt" \
  http://localhost:8000/stacker/admin/templates
```

---

## Phase 3: Marketplace Payment Flow Testing

**Goal:** Verify template approval webhooks and deployment validation

### 3.1 Create Test Template

```bash
# As regular user (OAuth token)
curl -X POST \
  -H "Authorization: Bearer $USER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Test Template",
    "slug": "test-template-'$(date +%s)'",
    "category_code": "databases",
    "version": "1.0.0"
  }' \
  http://localhost:8000/stacker/api/templates

# Response: 201 Created with template ID
TEMPLATE_ID="<from response>"
```

### 3.2 Approve Template (Triggers Webhook)

```bash
# As admin (JWT)
curl -X POST \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"decision": "approved"}' \
  http://localhost:8000/stacker/admin/templates/$TEMPLATE_ID/approve

# Check Stacker logs for webhook send:
docker compose logs stacker | grep -i webhook

# Check User Service received webhook:
docker compose logs user-service | grep "marketplace/sync"
```

### 3.3 Verify Product Created in User Service

```bash
# Query User Service product list
curl -H "Authorization: Bearer $USER_TOKEN" \
  http://localhost:4100/api/1.0/products

# Expected: Product for approved template appears in response
```

### 3.4 Test Deployment Validation

```bash
# 3.4a: Deploy free template (should work)
curl -X POST \
  -H "Authorization: Bearer $USER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"...": "..."}' \
  http://localhost:8000/stacker/api/projects/1/deploy

# Expected: 200 Success

# 3.4b: Deploy paid template without purchase (should fail)
# Update template to require "pro" plan
# Try to deploy as user without plan

# Expected: 403 Forbidden "You require a 'pro' subscription..."

# 3.4c: Purchase plan in User Service, retry deploy
# Deploy should succeed after purchase
```

---

## Success Criteria

### Phase 1 ✅
- [ ] Docker image builds without errors
- [ ] Container starts without panic
- [ ] Casbin rules are in database

### Phase 2 ✅
- [ ] Admin JWT token accepted: 200 OK
- [ ] Anonymous request rejected: 401
- [ ] Invalid token rejected: 401
- [ ] Expired token rejected: 401
- [ ] Correct Casbin rules returned from DB

### Phase 3 ✅
- [ ] Template approval sends webhook to User Service
- [ ] User Service creates product
- [ ] Product appears in `/api/1.0/products`
- [ ] Deployment validation enforces plan requirements
- [ ] Error messages are clear and actionable

---

## Debugging Commands

If tests fail, use these to diagnose:

```bash
# Check auth middleware logs
docker compose logs stacker | grep -i "jwt\|authentication\|acl"

# Check Casbin rule enforcement
docker compose logs stacker | grep "ACL check"

# Verify database state
docker exec stackerdb psql -U postgres -d stacker -c \
  "SELECT v0, v1, v2 FROM casbin_rule WHERE v0 LIKE '%admin%' ORDER BY id;"

# Check webhook payload in User Service
docker compose logs user-service | tail -50

# Test Casbin directly (if tool available)
docker exec stackerdb psql -U postgres -d stacker << SQL
SELECT * FROM casbin_rule WHERE v0='admin_service';
SQL
```

---

## Environment Setup

Before testing, ensure these are set:

```bash
# .env or export
export JWT_SECRET="your_secret_key"  # For future cryptographic validation
export USER_OAUTH_TOKEN="<valid_token_from_user_service>"
export ADMIN_JWT_TOKEN="<generated_above>"

# Verify services are running
docker compose ps
# Expected: stacker, stackerdb, user-service all running
```
