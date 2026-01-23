# Marketplace Plan Integration API Documentation

## Overview

Stacker's marketplace plan integration enables:
1. **Plan Validation** - Blocks deployments if user lacks required subscription tier
2. **Plan Discovery** - Exposes available plans for UI form population
3. **User Plan Verification** - Checks user's current plan status

All plan enforcement is done at **deployment time** - if a marketplace template requires a specific plan tier, the user must have that plan (or higher) to deploy it.

## Architecture

```
┌─────────────────┐
│  Stacker API    │
│  (Deployment)   │
└────────┬────────┘
         │
         ▼
┌──────────────────────────────────────┐
│   UserServiceConnector               │
│  - user_has_plan()                   │
│  - get_user_plan()                   │
│  - list_available_plans()            │
└────────┬──────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────┐
│  User Service API                    │
│  - /oauth_server/api/me              │
│  - /api/1.0/plan_description         │
└──────────────────────────────────────┘
```

## Endpoints

### 1. Deploy Project (with Plan Gating)

#### POST `/api/project/{id}/deploy`

Deploy a project. If the project was created from a marketplace template that requires a specific plan, the user must have that plan.

**Authentication**: Bearer token (OAuth) or HMAC

**Request**:
```bash
curl -X POST http://localhost:8000/api/project/123/deploy \
  -H "Authorization: Bearer <user_oauth_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "cloud_id": "5f4a2c1b-8e9d-4k2l-9m5n-3o6p7q8r9s0t"
  }'
```

**Request Body**:
```json
{
  "cloud_id": "cloud-provider-id"
}
```

**Response (Success - 200 OK)**:
```json
{
  "data": {
    "id": 123,
    "name": "My Project",
    "status": "deploying",
    "source_template_id": "uuid-of-marketplace-template",
    "template_version": "1.0.0"
  },
  "meta": {
    "status": "ok"
  }
}
```

**Response (Insufficient Plan - 403 Forbidden)**:
```json
{
  "error": "You require a 'professional' subscription to deploy this template",
  "status": "forbidden"
}
```

**Error Codes**:
| Code | Description |
|------|-------------|
| 200 | Deployment succeeded |
| 400 | Invalid cloud_id format |
| 403 | User lacks required plan for template |
| 404 | Project not found |
| 500 | Internal error (User Service unavailable) |

---

### 2. Get Available Plans (Admin)

#### GET `/api/admin/marketplace/plans`

List all available subscription plans from User Service. Used by admin UI to populate form dropdowns when creating/editing marketplace templates.

**Authentication**: Bearer token (OAuth) + Admin authorization

**Authorization**: Requires `group_admin` role (Casbin)

**Request**:
```bash
curl -X GET http://localhost:8000/api/admin/marketplace/plans \
  -H "Authorization: Bearer <admin_oauth_token>"
```

**Response (Success - 200 OK)**:
```json
{
  "data": [
    {
      "name": "basic",
      "description": "Basic Plan - Essential features",
      "tier": "basic",
      "features": {
        "deployments_per_month": 10,
        "team_members": 1,
        "api_access": false
      }
    },
    {
      "name": "professional",
      "description": "Professional Plan - Advanced features",
      "tier": "pro",
      "features": {
        "deployments_per_month": 50,
        "team_members": 5,
        "api_access": true
      }
    },
    {
      "name": "enterprise",
      "description": "Enterprise Plan - Full features",
      "tier": "enterprise",
      "features": {
        "deployments_per_month": null,
        "team_members": null,
        "api_access": true,
        "sso": true,
        "dedicated_support": true
      }
    }
  ],
  "meta": {
    "status": "ok"
  }
}
```

**Error Codes**:
| Code | Description |
|------|-------------|
| 200 | Plans retrieved successfully |
| 401 | Not authenticated |
| 403 | Not authorized (not admin) |
| 500 | User Service unavailable |

---

## Data Models

### StackTemplate (Marketplace Template)

**Table**: `stack_template`

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Template identifier |
| `creator_user_id` | String | User who created the template |
| `name` | String | Display name |
| `slug` | String | URL-friendly identifier |
| `category_id` | INT | Foreign key to `stack_category.id` |
| `product_id` | UUID | Product reference (created on approval) |
| `required_plan_name` | VARCHAR(50) NULL | Plan requirement: "basic", "professional", "enterprise", or NULL (no requirement) |
| `status` | ENUM | "draft", "submitted", "approved", "rejected" |
| `tags` | JSONB | Search tags |
| `tech_stack` | JSONB | Technologies used (e.g., ["nodejs", "postgresql"]) |
| `view_count` | INT NULL | Number of views |
| `deploy_count` | INT NULL | Number of deployments |
| `created_at` | TIMESTAMP NULL | Creation time |
| `updated_at` | TIMESTAMP NULL | Last update time |
| `average_rating` | FLOAT NULL | User rating (0-5) |

> **Category mirror note**: `stack_template.category_id` continues to store the numeric FK so we can reuse existing migrations and constraints. Runtime models expose `category_code` (the corresponding `stack_category.name`) for webhook payloads and API responses, so callers should treat `category_code` as the authoritative string identifier while leaving FK maintenance to the database layer.

### Project

**Table**: `project`

| Field | Type | Description |
|-------|------|-------------|
| `id` | INT | Project ID |
| `source_template_id` | UUID NULL | Links to `stack_template.id` if created from marketplace |
| `template_version` | VARCHAR NULL | Template version at creation time |
| ... | ... | Other project fields |

### PlanDefinition (from User Service)

```rust
pub struct PlanDefinition {
    pub name: String,              // "basic", "professional", "enterprise"
    pub description: Option<String>,
    pub tier: Option<String>,      // "basic", "pro", "enterprise"
    pub features: Option<serde_json::Value>,
}
```

### UserPlanInfo (from User Service)

```rust
pub struct UserPlanInfo {
    pub user_id: String,
    pub plan_name: String,         // User's current plan
    pub plan_description: Option<String>,
    pub tier: Option<String>,
    pub active: bool,
    pub started_at: Option<String>,
    pub expires_at: Option<String>,
}
```

---

## Plan Hierarchy

Plans are organized in a seniority order. Higher-tier users can access lower-tier templates:

```
┌─────────────┐
│ enterprise  │ ← Highest tier: Can deploy all templates
├─────────────┤
│ professional│ ← Mid tier: Can deploy professional & basic templates
├─────────────┤
│ basic       │ ← Low tier: Can only deploy basic templates
└─────────────┘
```

**Validation Logic** (implemented in `is_plan_upgrade()`):
```rust
fn user_has_plan(user_plan: &str, required_plan: &str) -> bool {
    if user_plan == required_plan {
        return true;  // Exact match
    }
    
    let hierarchy = vec!["basic", "professional", "enterprise"];
    let user_level = hierarchy.iter().position(|&p| p == user_plan).unwrap_or(0);
    let required_level = hierarchy.iter().position(|&p| p == required_plan).unwrap_or(0);
    
    user_level > required_level  // User's tier > required tier
}
```

**Examples**:
| User Plan | Required | Allowed? |
|-----------|----------|----------|
| basic | basic | ✅ Yes (equal) |
| professional | basic | ✅ Yes (higher tier) |
| enterprise | professional | ✅ Yes (higher tier) |
| basic | professional | ❌ No (insufficient) |
| professional | enterprise | ❌ No (insufficient) |

---

## User Service Integration

### Endpoints Used

#### 1. Get User's Current Plan
```
GET /oauth_server/api/me
Authorization: Bearer <user_oauth_token>
```

**Response**:
```json
{
  "plan": {
    "name": "professional",
    "date_end": "2026-01-30",
    "supported_stacks": {...},
    "deployments_left": 42
  }
}
```

#### 2. List Available Plans
```
GET /api/1.0/plan_description
Authorization: Bearer <auth_token>  (or Basic <base64(email:password)>)
```

**Response** (Eve REST API format):
```json
{
  "items": [
    {
      "name": "basic",
      "description": "Basic Plan",
      "tier": "basic",
      "features": {...}
    },
    ...
  ]
}
```

---

## Implementation Details

### Connector Pattern

All User Service communication goes through the `UserServiceConnector` trait:

**Location**: `src/connectors/user_service.rs`

```rust
#[async_trait::async_trait]
pub trait UserServiceConnector: Send + Sync {
    /// Check if user has access to a specific plan
    async fn user_has_plan(
        &self,
        user_id: &str,
        required_plan_name: &str,
    ) -> Result<bool, ConnectorError>;

    /// Get user's current plan information
    async fn get_user_plan(&self, user_id: &str) -> Result<UserPlanInfo, ConnectorError>;

    /// List all available plans
    async fn list_available_plans(&self) -> Result<Vec<PlanDefinition>, ConnectorError>;
}
```

### Production Implementation

Uses `UserServiceClient` - Makes actual HTTP requests to User Service.

### Testing Implementation

Uses `MockUserServiceConnector` - Returns hardcoded test data (always grants access).

**To use mock in tests**:
```rust
let connector: Arc<dyn UserServiceConnector> = Arc::new(MockUserServiceConnector);
// connector.user_has_plan(...) always returns Ok(true)
```

---

## Deployment Validation Flow

### Step-by-Step

1. **User calls**: `POST /api/project/{id}/deploy`
2. **Stacker fetches** project details from database
3. **Stacker checks** if project has `source_template_id`
4. **If yes**: Fetch template and check `required_plan_name`
5. **If required_plan set**: Call `user_service.user_has_plan(user_id, required_plan_name)`
6. **If false**: Return **403 Forbidden** with message
7. **If true**: Proceed with deployment (RabbitMQ publish, etc.)

### Code Location

**File**: `src/routes/project/deploy.rs`

**Methods**:
- `item()` - Deploy draft project (lines 16-86: plan validation logic)
- `saved_item()` - Deploy saved project (lines 207-276: plan validation logic)

**Validation snippet**:
```rust
if let Some(template_id) = project.source_template_id {
    if let Some(template) = db::marketplace::get_by_id(pg_pool.get_ref(), template_id).await? {
        if let Some(required_plan) = template.required_plan_name {
            let has_plan = user_service
                .user_has_plan(&user.id, &required_plan)
                .await?;
            
            if !has_plan {
                return Err(JsonResponse::build().forbidden(
                    format!("You require a '{}' subscription to deploy this template", required_plan),
                ));
            }
        }
    }
}
```

---

## Database Schema

### stack_template Table

```sql
CREATE TABLE stack_template (
    id UUID PRIMARY KEY,
    creator_user_id VARCHAR NOT NULL,
    name VARCHAR NOT NULL,
    slug VARCHAR NOT NULL UNIQUE,
    category_id UUID REFERENCES stack_category(id),
    product_id UUID REFERENCES product(id),
    required_plan_name VARCHAR(50),  -- NEW: Plan requirement
    status VARCHAR NOT NULL DEFAULT 'draft',
    tags JSONB,
    tech_stack JSONB,
    view_count INT,
    deploy_count INT,
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    average_rating FLOAT
);
```

### Migration Applied

**File**: `migrations/20251230_add_marketplace_required_plan.up.sql`

```sql
ALTER TABLE stack_template 
ADD COLUMN required_plan_name VARCHAR(50);
```

---

## Testing

### Unit Tests

**Location**: `src/routes/project/deploy.rs` (lines 370-537)

**Test Coverage**:
- ✅ User with required plan can deploy
- ✅ User without required plan is blocked
- ✅ User with higher tier plan can deploy
- ✅ Templates with no requirement allow any plan
- ✅ Plan hierarchy validation (basic < professional < enterprise)
- ✅ Mock connector grants access to all plans
- ✅ Mock connector returns correct plan list
- ✅ Mock connector returns user plan info

**Run tests**:
```bash
cargo test --lib routes::project::deploy
# Output: test result: ok. 9 passed; 0 failed
```

### Manual Testing (cURL)

```bash
# 1. Create template with plan requirement
curl -X POST http://localhost:8000/api/marketplace/templates \
  -H "Authorization: Bearer <creator_token>" \
  -d '{
    "name": "Premium App",
    "required_plan_name": "professional"
  }'

# 2. Try deployment as basic plan user → Should fail (403)
curl -X POST http://localhost:8000/api/project/123/deploy \
  -H "Authorization: Bearer <basic_plan_token>" \
  -d '{"cloud_id": "..."}'
# Response: 403 Forbidden - "You require a 'professional' subscription..."

# 3. Try deployment as professional plan user → Should succeed (200)
curl -X POST http://localhost:8000/api/project/123/deploy \
  -H "Authorization: Bearer <professional_plan_token>" \
  -d '{"cloud_id": "..."}'
# Response: 200 OK - Deployment started
```

---

## Error Handling

### Common Errors

| Scenario | HTTP Status | Response |
|----------|-------------|----------|
| User lacks required plan | 403 | `"You require a 'professional' subscription to deploy this template"` |
| User Service unavailable | 500 | `"Failed to validate subscription plan"` |
| Invalid cloud credentials | 400 | Form validation error |
| Project not found | 404 | `"not found"` |
| Unauthorized access | 401 | Not authenticated |

### Graceful Degradation

If User Service is temporarily unavailable:
1. Plan check fails with **500 Internal Server Error**
2. User sees message: "Failed to validate subscription plan"
3. Request **does not proceed** (fail-safe: deny deployment)

---

## Configuration

### Environment Variables

No special environment variables needed - uses existing User Service connector config.

**Configuration file**: `configuration.yaml`

```yaml
connectors:
  user_service:
    enabled: true
    base_url: "http://user:4100"
    timeout_secs: 10
    retry_attempts: 3
```

---

## Future Enhancements

1. **Payment Integration**: Add `/api/billing/start` endpoint to initiate payment
2. **Subscription Status**: User-facing endpoint to check current plan
3. **Plan Upgrade Prompts**: Frontend UI modal when deployment blocked
4. **Webhook Integration**: Receive plan change notifications from User Service
5. **Metrics**: Track plan-blocked deployments for analytics

---

## Support

**Questions?** Check:
- [DEVELOPERS.md](DEVELOPERS.md) - Development setup
- [TODO.md](TODO.md) - Overall roadmap
- [src/connectors/user_service.rs](../src/connectors/user_service.rs) - Implementation
- [src/routes/project/deploy.rs](../src/routes/project/deploy.rs) - Integration points
