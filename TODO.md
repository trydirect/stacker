# TODO: Stacker Marketplace Payment Integration

> Canonical note: keep all Stacker TODO updates in this file (`stacker/TODO.md`); do not create or update a separate `STACKER_TODO.md` going forward.

## Context
Per [PAYMENT_MODEL.md](/PAYMENT_MODEL.md), Stacker now sends webhooks to User Service when templates are published/updated. User Service owns the `products` table for monetization, while Stacker owns `stack_template` (template definitions only).

### New Open Questions (Status Panel & MCP)

**Status**: ✅ PROPOSED ANSWERS DOCUMENTED  
**See**: [OPEN_QUESTIONS_RESOLUTIONS.md](docs/OPEN_QUESTIONS_RESOLUTIONS.md)

**Questions** (awaiting team confirmation):
- Health check contract per app: exact URL/expected status/timeout that Status Panel should register and return.
- Per-app deploy trigger rate limits: allowed requests per minute/hour to expose in User Service.
- Log redaction patterns: which env var names/secret regexes to strip before returning logs via Stacker/User Service.
- Container→app_code mapping: confirm canonical source (deployment_apps.metadata.container_name) for Status Panel health/logs responses.

**Current Proposals**:
1. **Health Check**: `GET /api/health/deployment/{deployment_hash}/app/{app_code}` with 10s timeout
2. **Rate Limits**: Deploy 10/min, Restart 5/min, Logs 20/min (configurable by plan tier)
3. **Log Redaction**: 6 pattern categories + 20 env var blacklist (regex-based)
4. **Container Mapping**: `app_code` is canonical; requires `deployment_apps` table in User Service

### Status Panel Command Payloads (proposed)
- Commands flow over existing agent endpoints (`/api/v1/commands/execute` or `/enqueue`) signed with HMAC headers from `AgentClient`.
- **Health** request:
  ```json
  {"type":"health","deployment_hash":"<hash>","app_code":"<app>","include_metrics":true}
  ```
  **Health report** (agent → `/api/v1/commands/report`):
  ```json
  {"type":"health","deployment_hash":"<hash>","app_code":"<app>","status":"ok|unhealthy|unknown","container_state":"running|exited|starting|unknown","last_heartbeat_at":"2026-01-09T00:00:00Z","metrics":{"cpu_pct":0.12,"mem_mb":256},"errors":[]}
  ```
- **Logs** request:
  ```json
  {"type":"logs","deployment_hash":"<hash>","app_code":"<app>","cursor":"<opaque>","limit":400,"streams":["stdout","stderr"],"redact":true}
  ```
  **Logs report**:
  ```json
  {"type":"logs","deployment_hash":"<hash>","app_code":"<app>","cursor":"<next>","lines":[{"ts":"2026-01-09T00:00:00Z","stream":"stdout","message":"...","redacted":false}],"truncated":false}
  ```
- **Restart** request:
  ```json
  {"type":"restart","deployment_hash":"<hash>","app_code":"<app>","force":false}
  ```
  **Restart report**:
  ```json
  {"type":"restart","deployment_hash":"<hash>","app_code":"<app>","status":"ok|failed","container_state":"running|failed|unknown","errors":[]}
  ```
- Errors: agent reports `{ "type":"<same>", "deployment_hash":..., "app_code":..., "status":"failed", "errors":[{"code":"timeout","message":"..."}] }`.
- Tasks progress:
  1. ✅ add schemas/validation for these command payloads → implemented in `src/forms/status_panel.rs` and enforced via `/api/v1/commands` create/report handlers.
  2. ✅ document in agent docs → see `docs/AGENT_REGISTRATION_SPEC.md`, `docs/STACKER_INTEGRATION_REQUIREMENTS.md`, and `docs/QUICK_REFERENCE.md` (field reference + auth note).
  3. ✅ expose in Stacker UI/Status Panel integration notes → new `docs/STATUS_PANEL_INTEGRATION_NOTES.md` consumed by dashboard team.
  4. ⏳ ensure Vault token/HMAC headers remain the auth path (UI + ops playbook updates pending).

### Dynamic Agent Capabilities Endpoint
- [x] Expose `GET /api/v1/deployments/{deployment_hash}/capabilities` returning available commands based on `agents.capabilities` JSONB (implemented in `routes::deployment::capabilities_handler`).
- [x] Define command→capability mapping (static config) embedded in the handler:
  ```json
  {
    "restart": { "requires": "docker", "scope": "container", "label": "Restart", "icon": "fas fa-redo" },
    "start":   { "requires": "docker", "scope": "container", "label": "Start", "icon": "fas fa-play" },
    "stop":    { "requires": "docker", "scope": "container", "label": "Stop", "icon": "fas fa-stop" },
    "pause":   { "requires": "docker", "scope": "container", "label": "Pause", "icon": "fas fa-pause" },
    "logs":    { "requires": "logs",   "scope": "container", "label": "Logs", "icon": "fas fa-file-alt" },
    "rebuild": { "requires": "compose", "scope": "deployment", "label": "Rebuild Stack", "icon": "fas fa-sync" },
    "backup":  { "requires": "backup", "scope": "deployment", "label": "Backup", "icon": "fas fa-download" }
  }
  ```
- [x] Return only commands whose `requires` capability is present in the agent's capabilities array (see `filter_commands` helper).
- [x] Include agent status (online/offline) and last_heartbeat plus existing metadata in the response so Blog can gate UI.

### Pull-Only Command Architecture (No Push)
**Key principle**: Stacker never dials out to agents. Commands are enqueued in the database; agents poll and sign their own requests.
- [x] `POST /api/v1/agent/commands/enqueue` validates user auth, inserts into `commands` + `command_queue` tables, returns 202. No outbound HTTP to agent.
- [x] Agent polls `GET /api/v1/agent/commands/wait/{deployment_hash}` with HMAC headers it generates using its Vault-fetched token.
- [x] Stacker verifies agent's HMAC, returns queued commands.
- [x] Agent executes locally and calls `POST /api/v1/agent/commands/report` (HMAC-signed).
- [x] Remove any legacy `agent_dispatcher::execute/enqueue` code that attempted to push to agents; keep only `rotate_token` for Vault token management.
- [x] Document that `AGENT_BASE_URL` env var is NOT required for Status Panel; Stacker is server-only (see README.md).

### Dual Endpoint Strategy (Status Panel + Compose Agent)
- [ ] Maintain legacy proxy routes under `/api/v1/deployments/{hash}/containers/*` for hosts without Compose Agent; ensure regression tests continue to cover restart/start/stop/logs flows.
- [ ] Add Compose control-plane routes (`/api/v1/compose/{hash}/status|logs|restart|metrics`) that translate into cagent API calls using the new `compose_agent_token` from Vault.
- [ ] For Compose Agent path only: `agent_dispatcher` may push commands if cagent exposes an HTTP API; this is the exception, not the rule.
- [ ] Return `"compose_agent": true|false` in `/capabilities` response plus a `"fallback_reason"` field when Compose Agent is unavailable (missing registration, unhealthy heartbeat, token fetch failure).
- [ ] Write ops playbook entry + automated alert when Compose Agent is offline for >15 minutes so we can investigate hosts stuck on the legacy path.

### Coordination Note
Sub-agents can communicate with the team lead via the shared memory tool (see /memories/subagents.md). If questions remain, record them in TODO.md and log work in CHANGELOG.md.

### Nginx Proxy Routing
**Browser → Stacker** (via nginx): `https://dev.try.direct/stacker/` → `stacker:8000`
**Stacker → User Service** (internal): `http://user:4100/marketplace/sync` (no nginx prefix)
**Stacker → Payment Service** (internal): `http://payment:8000/` (no nginx prefix)

Stacker responsibilities:
1. **Maintain `stack_template` table** (template definitions, no pricing/monetization)
2. **Send webhook to User Service** when template status changes (approved, updated, rejected)
3. **Query User Service** for product information (pricing, vendor, etc.)
4. **Validate deployments** against User Service product ownership

## Improvements
### Top improvements
- [x] Cache OAuth token validation in Stacker (30–60s TTL) to avoid a User Service call on every request.
- [x] Reuse/persist the HTTP client with keep-alive and a shared connection pool for User Service; avoid starting new connections per request.
- [x] Stop reloading Casbin policies on every request; reload on policy change.
- [x] Reduce polling frequency and batch command status queries; prefer streaming/long-poll responses.
- [ ] Add server-side aggregation: return only latest command states instead of fetching full 150+ rows each time.
- [x] Add gzip/br on internal HTTP responses and trim response payloads.
- [x] Co-locate Stacker and User Service (same network/region) or use private networking to cut latency.

### Backlog hygiene
- [ ] Capture ongoing UX friction points from Stack Builder usage and log them here.
- [ ] Track recurring operational pain points (timeouts, retries, auth failures) for batch fixes.
- [ ] Record documentation gaps that slow down onboarding or integration work.

## Tasks

### Data Contract Notes (2026-01-04)
- `project_id` in Stacker is the same identifier as `stack_id` in the User Service `installation` table; use it to link records across services.
- Include `deployment_hash` from Stacker in payloads sent to Install Service (RabbitMQ) and User Service so both can track deployments by the unique deployment key. Coordinate with try.direct.tools to propagate this field through shared publishers/helpers.

### 0. Setup ACL Rules Migration (User Service)
**File**: `migrations/setup_acl_rules.py` (in Stacker repo)

**Purpose**: Automatically configure Casbin ACL rules in User Service for Stacker endpoints

**Required Casbin rules** (to be inserted in User Service `casbin_rule` table):
```python
# Allow root/admin to manage marketplace templates via Stacker
rules = [
    ('p', 'root', '/templates', 'POST', '', '', ''),      # Create template
    ('p', 'root', '/templates', 'GET', '', '', ''),       # List templates
    ('p', 'root', '/templates/*', 'GET', '', '', ''),     # View template
    ('p', 'root', '/templates/*', 'PUT', '', '', ''),     # Update template
    ('p', 'root', '/templates/*', 'DELETE', '', '', ''),  # Delete template
    ('p', 'admin', '/templates', 'POST', '', '', ''),
    ('p', 'admin', '/templates', 'GET', '', '', ''),
    ('p', 'admin', '/templates/*', 'GET', '', '', ''),
    ('p', 'admin', '/templates/*', 'PUT', '', '', ''),
    ('p', 'developer', '/templates', 'POST', '', '', ''),  # Developers can create
    ('p', 'developer', '/templates', 'GET', '', '', ''),   # Developers can list own
]
```

**Implementation**:
- Run as part of Stacker setup/init
- Connect to User Service database
- Insert rules if not exist (idempotent)
- **Status**: NOT STARTED
- **Priority**: HIGH (Blocks template creation via Stack Builder)
- **ETA**: 30 minutes

### 0.5. Add Category Table Fields & Sync (Stacker)
**File**: `migrations/add_category_fields.py` (in Stacker repo)

**Purpose**: Add missing fields to Stacker's local `category` table and sync from User Service

**Migration Steps**:
1. Add `title VARCHAR(255)` column to `category` table (currently only has `id`, `name`)
2. Add `metadata JSONB` column for flexible category data
3. Create `UserServiceConnector.sync_categories()` method
4. On application startup: Fetch categories from User Service `GET http://user:4100/api/1.0/category`
5. Populate/update local `category` table:
   - Map User Service `name` → Stacker `name` (code)
   - Map User Service `title` → Stacker `title`
   - Store additional data in `metadata` JSONB

**Example sync**:
```python
# User Service category
{"_id": 5, "name": "ai", "title": "AI Agents", "priority": 5}

# Stacker local category (after sync)
{"id": 5, "name": "ai", "title": "AI Agents", "metadata": {"priority": 5}}
```

**Status**: NOT STARTED  
**Priority**: HIGH (Required for Stack Builder UI)  
**ETA**: 1 hour

### 1. Create User Service Connector
**File**: `app/<stacker-module>/connectors/user_service_connector.py` (in Stacker repo)

**Required methods**:
```python
class UserServiceConnector:
    def get_categories(self) -> list:
        """
        GET http://user:4100/api/1.0/category
        
        Returns list of available categories for stack classification:
        [
            {"_id": 1, "name": "cms", "title": "CMS", "priority": 1},
            {"_id": 2, "name": "ecommerce", "title": "E-commerce", "priority": 2},
            {"_id": 5, "name": "ai", "title": "AI Agents", "priority": 5}
        ]
        
        Used by: Stack Builder UI to populate category dropdown
        """
        pass
    
    def get_user_profile(self, user_token: str) -> dict:
        """
        GET http://user:4100/oauth_server/api/me
        Headers: Authorization: Bearer {user_token}
        
        Returns:
        {
            "email": "user@example.com",
            "plan": {
                "name": "plus",
                "date_end": "2026-01-30"
            },
            "products": [
                {
                    "product_id": "uuid",
                    "product_type": "template",
                    "code": "ai-agent-stack",
                    "external_id": 12345,  # stack_template.id from Stacker
                    "name": "AI Agent Stack",
                    "price": "99.99",
                    "owned_since": "2025-01-15T..."
                }
            ]
        }
        """
        pass
    
    def get_template_product(self, stack_template_id: int) -> dict:
        """
        GET http://user:4100/api/1.0/products?external_id={stack_template_id}&product_type=template
        
        Returns product info for a marketplace template (pricing, vendor, etc.)
        """
        pass
    
    def user_owns_template(self, user_token: str, stack_template_id: int) -> bool:
        """
        Check if user has purchased/owns this marketplace template
        """
        profile = self.get_user_profile(user_token)
        return any(p['external_id'] == stack_template_id and p['product_type'] == 'template' 
                   for p in profile.get('products', []))
```

**Implementation Note**: Use OAuth2 token that Stacker already has for the user.

### 2. Create Webhook Sender to User Service (Marketplace Sync)
**File**: `app/<stacker-module>/webhooks/marketplace_webhook.py` (in Stacker repo)

**When template status changes** (approved, updated, rejected):
```python
import requests
from os import environ

class MarketplaceWebhookSender:
    """
    Send template sync webhooks to User Service
    Mirrors PAYMENT_MODEL.md Flow 3: Stacker template changes → User Service products
    """
    
    def send_template_approved(self, stack_template: dict, vendor_user: dict):
        """
        POST http://user:4100/marketplace/sync
        
        Body:
        {
            "action": "template_approved",
            "stack_template_id": 12345,
            "external_id": 12345,  # Same as stack_template_id
            "code": "ai-agent-stack-pro",
            "name": "AI Agent Stack Pro",
            "description": "Advanced AI agent deployment...",
            "category_code": "ai",  # String code from local category.name (not ID)
            "price": 99.99,
            "billing_cycle": "one_time",  # or "monthly"
            "currency": "USD",
            "vendor_user_id": 456,
            "vendor_name": "John Doe"
        }
        """
        headers = {'Authorization': f'Bearer {self.get_service_token()}'}
        
        payload = {
            'action': 'template_approved',
            'stack_template_id': stack_template['id'],
            'external_id': stack_template['id'],
            'code': stack_template.get('code'),
            'name': stack_template.get('name'),
            'description': stack_template.get('description'),
            'category_code': stack_template.get('category'),  # String code (e.g., "ai", "cms")
            'price': stack_template.get('price'),
            'billing_cycle': stack_template.get('billing_cycle', 'one_time'),
            'currency': stack_template.get('currency', 'USD'),
            'vendor_user_id': vendor_user['id'],
            'vendor_name': vendor_user.get('full_name', vendor_user.get('email'))
        }
        
        response = requests.post(
            f"{environ['URL_SERVER_USER']}/marketplace/sync",
            json=payload,
            headers=headers
        )
        
        if response.status_code != 200:
            raise Exception(f"Webhook send failed: {response.text}")
        
        return response.json()
    
    def send_template_updated(self, stack_template: dict, vendor_user: dict):
        """Send template updated webhook (same format as approved)"""
        payload = {...}
        payload['action'] = 'template_updated'
        # Send like send_template_approved()
    
    def send_template_rejected(self, stack_template: dict):
        """
        Notify User Service to deactivate product
        
        Body:
        {
            "action": "template_rejected",
            "stack_template_id": 12345
        }
        """
        headers = {'Authorization': f'Bearer {self.get_service_token()}'}
        
        payload = {
            'action': 'template_rejected',
            'stack_template_id': stack_template['id']
        }
        
        response = requests.post(
            f"{environ['URL_SERVER_USER']}/marketplace/sync",
            json=payload,
            headers=headers
        )
        
        return response.json()
    
    @staticmethod
    def get_service_token() -> str:
        """Get Bearer token for service-to-service communication"""
        # Option 1: Use static bearer token
        return environ.get('STACKER_SERVICE_TOKEN')
        
        # Option 2: Use OAuth2 client credentials flow (preferred)
        # See User Service `.github/copilot-instructions.md` for setup
```

**Integration points** (where to call webhook sender):

1. **When template is approved by admin**:
```python
def approve_template(template_id: int):
    template = StackTemplate.query.get(template_id)
    vendor = User.query.get(template.created_by_user_id)
    template.status = 'approved'
    db.session.commit()
    
    # Send webhook to User Service to create product
    webhook_sender = MarketplaceWebhookSender()
    webhook_sender.send_template_approved(template.to_dict(), vendor.to_dict())
```

2. **When template is updated**:
```python
def update_template(template_id: int, updates: dict):
    template = StackTemplate.query.get(template_id)
    template.update(updates)
    db.session.commit()
    
    if template.status == 'approved':
        vendor = User.query.get(template.created_by_user_id)
        webhook_sender = MarketplaceWebhookSender()
        webhook_sender.send_template_updated(template.to_dict(), vendor.to_dict())
```

3. **When template is rejected**:
```python
def reject_template(template_id: int):
    template = StackTemplate.query.get(template_id)
    template.status = 'rejected'
    db.session.commit()
    
    webhook_sender = MarketplaceWebhookSender()
    webhook_sender.send_template_rejected(template.to_dict())
```

### 3. Add Deployment Validation
**File**: `app/<stacker-module>/services/deployment_service.py` (update existing)

**Before allowing deployment, validate**:
```python
from .connectors.user_service_connector import UserServiceConnector

class DeploymentValidator:
    def validate_marketplace_template(self, stack_template: dict, user_token: str):
        """
        Check if user can deploy this marketplace template
        
        If template has a product in User Service:
        - Check if user owns product (in user_products table)
        - If not owned, block deployment
        """
        connector = UserServiceConnector()
        
        # If template is not marketplace template, allow deployment
        if not stack_template.get('is_from_marketplace'):
            return True
        
        # Check if template has associated product
        template_id = stack_template['id']
        product_info = connector.get_template_product(template_id)
        
        if not product_info:
            # No product = free marketplace template, allow deployment
            return True
        
        # Check if user owns this template product
        user_owns = connector.user_owns_template(user_token, template_id)
        
        if not user_owns:
            raise TemplateNotPurchasedError(
                f"This verified pro stack requires purchase. "
                f"Price: ${product_info.get('price')}. "
                f"Please purchase from User Service."
            )
        
        return True
```

**Integrate into deployment flow**:
```python
def start_deployment(template_id: int, user_token: str):
    template = StackTemplate.query.get(template_id)
    
    # Validate permission to deploy this template
    validator = DeploymentValidator()
    validator.validate_marketplace_template(template.to_dict(), user_token)
    
    # Continue with deployment...
```

## Environment Variables Needed (Stacker)
Add to Stacker's `.env`:
```bash
# User Service
URL_SERVER_USER=http://user:4100/

# Service-to-service auth token (for webhook sender)
STACKER_SERVICE_TOKEN=<bearer-token-from-user-service>

# Or use OAuth2 client credentials (preferred)
STACKER_CLIENT_ID=<from-user-service>
STACKER_CLIENT_SECRET=<from-user-service>
```

## Testing Checklist

### Unit Tests
- [ ] `test_user_service_connector.py`:
  - [ ] `get_user_profile()` returns user with products list
  - [ ] `get_template_product()` returns product info
  - [ ] `user_owns_template()` returns correct boolean
- [ ] `test_marketplace_webhook_sender.py`:
  - [ ] `send_template_approved()` sends correct webhook payload
  - [ ] `send_template_updated()` sends correct webhook payload
  - [ ] `send_template_rejected()` sends correct webhook payload
  - [ ] `get_service_token()` returns valid bearer token
- [ ] `test_deployment_validator.py`:
  - [ ] `validate_marketplace_template()` allows free templates
  - [ ] `validate_marketplace_template()` allows user-owned paid templates
  - [ ] `validate_marketplace_template()` blocks non-owned paid templates
  - [ ] Raises `TemplateNotPurchasedError` with correct message

### Integration Tests
- [ ] `test_template_approval_flow.py`:
  - [ ] Admin approves template in Stacker
  - [ ] Webhook sent to User Service `/marketplace/sync`
  - [ ] User Service creates product
  - [ ] `/oauth_server/api/me` includes new product
- [ ] `test_template_update_flow.py`:
  - [ ] Vendor updates template in Stacker
  - [ ] Webhook sent to User Service
  - [ ] Product updated in User Service
- [ ] `test_template_rejection_flow.py`:
  - [ ] Admin rejects template
  - [ ] Webhook sent to User Service
  - [ ] Product deactivated in User Service
- [ ] `test_deployment_validation_flow.py`:
  - [ ] User can deploy free marketplace template
  - [ ] User cannot deploy paid template without purchase
  - [ ] User can deploy paid template after product purchase
  - [ ] Correct error messages in each scenario

### Manual Testing
- [ ] Stacker can query User Service `/oauth_server/api/me` (with real user token)
- [ ] Stacker connector returns user profile with products list
- [ ] Approve template in Stacker admin → webhook sent to User Service
- [ ] User Service `/marketplace/sync` creates product
- [ ] Product appears in `/api/1.0/products` endpoint
- [ ] Deployment validation blocks unpurchased paid templates
- [ ] Deployment validation allows owned paid templates
- [ ] All environment variables configured correctly

## Coordination

**Dependencies**:
1. ✅ User Service - `/marketplace/sync` webhook endpoint (created in User Service TODO)
2. ✅ User Service - `products` + `user_products` tables (created in User Service TODO)
3. ⏳ Stacker - User Service connector + webhook sender (THIS TODO)
4. ✅ Payment Service - No changes needed (handles all webhooks same way)

**Service Interaction Flow**:

```
Vendor Creates Template in Stacker
  ↓
Admin Approves in Stacker
  ↓
Stacker calls MarketplaceWebhookSender.send_template_approved()
  ↓
POST http://user:4100/marketplace/sync
  {
    "action": "template_approved",
    "stack_template_id": 12345,
    "price": 99.99,
    "vendor_user_id": 456,
    ...
  }
  ↓
User Service creates `products` row
  (product_type='template', external_id=12345, vendor_id=456, price=99.99)
  ↓
Template now available in User Service `/api/1.0/products?product_type=template`
  ↓
Blog queries User Service for marketplace templates
  ↓
User views template in marketplace, clicks "Deploy"
  ↓
User pays (Payment Service handles all payment flows)
  ↓
Payment Service webhook → User Service (adds row to `user_products`)
  ↓
Stacker queries User Service `/oauth_server/api/me`
  ↓
User Service returns products list (includes newly purchased template)
  ↓
DeploymentValidator.validate_marketplace_template() checks ownership
  ↓
Deployment proceeds (user owns product)
```

## Notes

**Architecture Decisions**:
1. Stacker only sends webhooks to User Service (no bi-directional queries)
2. User Service owns monetization logic (products table)
3. Payment Service forwards webhooks to User Service (same handler for all product types)
4. `stack_template.id` (Stacker) links to `products.external_id` (User Service) via webhook
5. Deployment validation queries User Service for product ownership

**Key Points**:
- DO NOT store pricing in Stacker `stack_template` table
- DO NOT create products table in Stacker (they're in User Service)
- DO send webhooks to User Service when template status changes
- DO use Bearer token for service-to-service auth in webhooks
- Webhook sender is simpler than Stacker querying User Service (one-way communication)

## Timeline Estimate

- Phase 1 (User Service connector): 1-2 hours
- Phase 2 (Webhook sender): 1-2 hours
- Phase 3 (Deployment validation): 1-2 hours
- Phase 4 (Testing): 3-4 hours
- **Total**: 6-10 hours (~1 day)

## Reference Files
- [PAYMENT_MODEL.md](/PAYMENT_MODEL.md) - Architecture
- [try.direct.user.service/TODO.md](try.direct.user.service/TODO.md) - User Service implementation
- [try.direct.tools/TODO.md](try.direct.tools/TODO.md) - Shared utilities
- [blog/TODO.md](blog/TODO.md) - Frontend marketplace UI

---

## Synced copy from /STACKER_TODO.md (2026-01-03)

# TODO: Stacker Marketplace Payment Integration

## Context
Per [PAYMENT_MODEL.md](/PAYMENT_MODEL.md), Stacker now sends webhooks to User Service when templates are published/updated. User Service owns the `products` table for monetization, while Stacker owns `stack_template` (template definitions only).

Stacker responsibilities:
1. **Maintain `stack_template` table** (template definitions, no pricing/monetization)
2. **Send webhook to User Service** when template status changes (approved, updated, rejected)
3. **Query User Service** for product information (pricing, vendor, etc.)
4. **Validate deployments** against User Service product ownership

## Tasks

### Bugfix: Return clear duplicate slug error
- [ ] When `stack_template.slug` violates uniqueness (code 23505), return 409/400 with a descriptive message (e.g., "slug already exists") instead of 500 so clients (blog/stack-builder) can surface a user-friendly error.

### 1. Create User Service Connector
**File**: `app/<stacker-module>/connectors/user_service_connector.py` (in Stacker repo)

**Required methods**:
```python
class UserServiceConnector:
    def get_user_profile(self, user_token: str) -> dict:
        """
        GET http://user:4100/oauth_server/api/me
        Headers: Authorization: Bearer {user_token}
        
        Returns:
        {
            "email": "user@example.com",
            "plan": {
                "name": "plus",
                "date_end": "2026-01-30"
            },
            "products": [
                {
                    "product_id": "uuid",
                    "product_type": "template",
                    "code": "ai-agent-stack",
                    "external_id": 12345,  # stack_template.id from Stacker
                    "name": "AI Agent Stack",
                    "price": "99.99",
                    "owned_since": "2025-01-15T..."
                }
            ]
        }
        """
        pass
    
    def get_template_product(self, stack_template_id: int) -> dict:
        """
        GET http://user:4100/api/1.0/products?external_id={stack_template_id}&product_type=template
        
        Returns product info for a marketplace template (pricing, vendor, etc.)
        """
        pass
    
    def user_owns_template(self, user_token: str, stack_template_id: int) -> bool:
        """
        Check if user has purchased/owns this marketplace template
        """
        profile = self.get_user_profile(user_token)
        return any(p['external_id'] == stack_template_id and p['product_type'] == 'template' 
                   for p in profile.get('products', []))
```

**Implementation Note**: Use OAuth2 token that Stacker already has for the user.

### 2. Create Webhook Sender to User Service (Marketplace Sync)
**File**: `app/<stacker-module>/webhooks/marketplace_webhook.py` (in Stacker repo)

**When template status changes** (approved, updated, rejected):
```python
import requests
from os import environ

class MarketplaceWebhookSender:
    """
    Send template sync webhooks to User Service
    Mirrors PAYMENT_MODEL.md Flow 3: Stacker template changes → User Service products
    """
    
    def send_template_approved(self, stack_template: dict, vendor_user: dict):
        """
        POST http://user:4100/marketplace/sync
        
        Body:
        {
            "action": "template_approved",
            "stack_template_id": 12345,
            "external_id": 12345,  # Same as stack_template_id
            "code": "ai-agent-stack-pro",
            "name": "AI Agent Stack Pro",
            "description": "Advanced AI agent deployment...",
            "price": 99.99,
            "billing_cycle": "one_time",  # or "monthly"
            "currency": "USD",
            "vendor_user_id": 456,
            "vendor_name": "John Doe"
        }
        """
        headers = {'Authorization': f'Bearer {self.get_service_token()}'}
        
        payload = {
            'action': 'template_approved',
            'stack_template_id': stack_template['id'],
            'external_id': stack_template['id'],
            'code': stack_template.get('code'),
            'name': stack_template.get('name'),
            'description': stack_template.get('description'),
            'price': stack_template.get('price'),
            'billing_cycle': stack_template.get('billing_cycle', 'one_time'),
            'currency': stack_template.get('currency', 'USD'),
            'vendor_user_id': vendor_user['id'],
            'vendor_name': vendor_user.get('full_name', vendor_user.get('email'))
        }
        
        response = requests.post(
            f"{environ['URL_SERVER_USER']}/marketplace/sync",
            json=payload,
            headers=headers
        )
        
        if response.status_code != 200:
            raise Exception(f"Webhook send failed: {response.text}")
        
        return response.json()
    
    def send_template_updated(self, stack_template: dict, vendor_user: dict):
        """Send template updated webhook (same format as approved)"""
        payload = {...}
        payload['action'] = 'template_updated'
        # Send like send_template_approved()
    
    def send_template_rejected(self, stack_template: dict):
        """
        Notify User Service to deactivate product
        
        Body:
        {
            "action": "template_rejected",
            "stack_template_id": 12345
        }
        """
        headers = {'Authorization': f'Bearer {self.get_service_token()}'}
        
        payload = {
            'action': 'template_rejected',
            'stack_template_id': stack_template['id']
        }
        
        response = requests.post(
            f"{environ['URL_SERVER_USER']}/marketplace/sync",
            json=payload,
            headers=headers
        )
        
        return response.json()
    
    @staticmethod
    def get_service_token() -> str:
        """Get Bearer token for service-to-service communication"""
        # Option 1: Use static bearer token
        return environ.get('STACKER_SERVICE_TOKEN')
        
        # Option 2: Use OAuth2 client credentials flow (preferred)
        # See User Service `.github/copilot-instructions.md` for setup
```

**Integration points** (where to call webhook sender):

1. **When template is approved by admin**:
```python
def approve_template(template_id: int):
    template = StackTemplate.query.get(template_id)
    vendor = User.query.get(template.created_by_user_id)
    template.status = 'approved'
    db.session.commit()
    
    # Send webhook to User Service to create product
    webhook_sender = MarketplaceWebhookSender()
    webhook_sender.send_template_approved(template.to_dict(), vendor.to_dict())
```

2. **When template is updated**:
```python
def update_template(template_id: int, updates: dict):
    template = StackTemplate.query.get(template_id)
    template.update(updates)
    db.session.commit()
    
    if template.status == 'approved':
        vendor = User.query.get(template.created_by_user_id)
        webhook_sender = MarketplaceWebhookSender()
        webhook_sender.send_template_updated(template.to_dict(), vendor.to_dict())
```

3. **When template is rejected**:
```python
def reject_template(template_id: int):
    template = StackTemplate.query.get(template_id)
    template.status = 'rejected'
    db.session.commit()
    
    webhook_sender = MarketplaceWebhookSender()
    webhook_sender.send_template_rejected(template.to_dict())
```

### 3. Add Deployment Validation
**File**: `app/<stacker-module>/services/deployment_service.py` (update existing)

**Before allowing deployment, validate**:
```python
from .connectors.user_service_connector import UserServiceConnector

class DeploymentValidator:
    def validate_marketplace_template(self, stack_template: dict, user_token: str):
        """
        Check if user can deploy this marketplace template
        
        If template has a product in User Service:
        - Check if user owns product (in user_products table)
        - If not owned, block deployment
        """
        connector = UserServiceConnector()
        
        # If template is not marketplace template, allow deployment
        if not stack_template.get('is_from_marketplace'):
            return True
        
        # Check if template has associated product
        template_id = stack_template['id']
        product_info = connector.get_template_product(template_id)
        
        if not product_info:
            # No product = free marketplace template, allow deployment
            return True
        
        # Check if user owns this template product
        user_owns = connector.user_owns_template(user_token, template_id)
        
        if not user_owns:
            raise TemplateNotPurchasedError(
                f"This verified pro stack requires purchase. "
                f"Price: ${product_info.get('price')}. "
                f"Please purchase from User Service."
            )
        
        return True
```

**Integrate into deployment flow**:
```python
def start_deployment(template_id: int, user_token: str):
    template = StackTemplate.query.get(template_id)
    
    # Validate permission to deploy this template
    validator = DeploymentValidator()
    validator.validate_marketplace_template(template.to_dict(), user_token)
    
    # Continue with deployment...
```

## Environment Variables Needed (Stacker)
Add to Stacker's `.env`:
```bash
# User Service
URL_SERVER_USER=http://user:4100/

# Service-to-service auth token (for webhook sender)
STACKER_SERVICE_TOKEN=<bearer-token-from-user-service>

# Or use OAuth2 client credentials (preferred)
STACKER_CLIENT_ID=<from-user-service>
STACKER_CLIENT_SECRET=<from-user-service>
```

## Testing Checklist

### Unit Tests
- [ ] `test_user_service_connector.py`:
  - [ ] `get_user_profile()` returns user with products list
  - [ ] `get_template_product()` returns product info
  - [ ] `user_owns_template()` returns correct boolean
- [ ] `test_marketplace_webhook_sender.py`:
  - [ ] `send_template_approved()` sends correct webhook payload
  - [ ] `send_template_updated()` sends correct webhook payload
  - [ ] `send_template_rejected()` sends correct webhook payload
  - [ ] `get_service_token()` returns valid bearer token
- [ ] `test_deployment_validator.py`:
  - [ ] `validate_marketplace_template()` allows free templates
  - [ ] `validate_marketplace_template()` allows user-owned paid templates
  - [ ] `validate_marketplace_template()` blocks non-owned paid templates
  - [ ] Raises `TemplateNotPurchasedError` with correct message

### Integration Tests
- [ ] `test_template_approval_flow.py`:
  - [ ] Admin approves template in Stacker
  - [ ] Webhook sent to User Service `/marketplace/sync`
  - [ ] User Service creates product
  - [ ] `/oauth_server/api/me` includes new product
- [ ] `test_template_update_flow.py`:
  - [ ] Vendor updates template in Stacker
  - [ ] Webhook sent to User Service
  - [ ] Product updated in User Service
- [ ] `test_template_rejection_flow.py`:
  - [ ] Admin rejects template
  - [ ] Webhook sent to User Service
  - [ ] Product deactivated in User Service
- [ ] `test_deployment_validation_flow.py`:
  - [ ] User can deploy free marketplace template
  - [ ] User cannot deploy paid template without purchase
  - [ ] User can deploy paid template after product purchase
  - [ ] Correct error messages in each scenario

### Manual Testing
- [ ] Stacker can query User Service `/oauth_server/api/me` (with real user token)
- [ ] Stacker connector returns user profile with products list
- [ ] Approve template in Stacker admin → webhook sent to User Service
- [ ] User Service `/marketplace/sync` creates product
- [ ] Product appears in `/api/1.0/products` endpoint
- [ ] Deployment validation blocks unpurchased paid templates
- [ ] Deployment validation allows owned paid templates
- [ ] All environment variables configured correctly

## Coordination

**Dependencies**:
1. ✅ User Service - `/marketplace/sync` webhook endpoint (created in User Service TODO)
2. ✅ User Service - `products` + `user_products` tables (created in User Service TODO)
3. ⏳ Stacker - User Service connector + webhook sender (THIS TODO)
4. ✅ Payment Service - No changes needed (handles all webhooks same way)

**Service Interaction Flow**:

```
Vendor Creates Template in Stacker
  ↓
Admin Approves in Stacker
  ↓
Stacker calls MarketplaceWebhookSender.send_template_approved()
  ↓
POST http://user:4100/marketplace/sync
  {
    "action": "template_approved",
    "stack_template_id": 12345,
    "price": 99.99,
    "vendor_user_id": 456,
    ...
  }
  ↓
User Service creates `products` row
  (product_type='template', external_id=12345, vendor_id=456, price=99.99)
  ↓
Template now available in User Service `/api/1.0/products?product_type=template`
  ↓
Blog queries User Service for marketplace templates
  ↓
User views template in marketplace, clicks "Deploy"
  ↓
User pays (Payment Service handles all payment flows)
  ↓
Payment Service webhook → User Service (adds row to `user_products`)
  ↓
Stacker queries User Service `/oauth_server/api/me`
  ↓
User Service returns products list (includes newly purchased template)
  ↓
DeploymentValidator.validate_marketplace_template() checks ownership
  ↓
Deployment proceeds (user owns product)
```

## Notes

**Architecture Decisions**:
1. Stacker only sends webhooks to User Service (no bi-directional queries)
2. User Service owns monetization logic (products table)
3. Payment Service forwards webhooks to User Service (same handler for all product types)
4. `stack_template.id` (Stacker) links to `products.external_id` (User Service) via webhook
5. Deployment validation queries User Service for product ownership

**Key Points**:
- DO NOT store pricing in Stacker `stack_template` table
- DO NOT create products table in Stacker (they're in User Service)
- DO send webhooks to User Service when template status changes
- DO use Bearer token for service-to-service auth in webhooks
- Webhook sender is simpler than Stacker querying User Service (one-way communication)

## Timeline Estimate

- Phase 1 (User Service connector): 1-2 hours
- Phase 2 (Webhook sender): 1-2 hours
- Phase 3 (Deployment validation): 1-2 hours
- Phase 4 (Testing): 3-4 hours
- **Total**: 6-10 hours (~1 day)

## Reference Files
- [PAYMENT_MODEL.md](/PAYMENT_MODEL.md) - Architecture
- [try.direct.user.service/TODO.md](try.direct.user.service/TODO.md) - User Service implementation
- [try.direct.tools/TODO.md](try.direct.tools/TODO.md) - Shared utilities
- [blog/TODO.md](blog/TODO.md) - Frontend marketplace UI
