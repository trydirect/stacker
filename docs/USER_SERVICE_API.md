# Try.Direct User Service - API Endpoints Reference

All endpoints are prefixed with `/server/user` (set via `WEB_SERVER_PREFIX` in config.py).

## Authentication (`/auth`)

User registration, login, password recovery, and account management endpoints.

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| POST | `/auth/login` | Email & password login, returns OAuth tokens | No | 1/second |
| POST | `/auth/register` | New user registration | No | 8/minute |
| POST | `/auth/change_email` | Change unconfirmed email | Yes | No limit |
| POST | `/auth/confirmation/send` | Send confirmation email to new user | No | 1/6 min |
| POST | `/auth/confirmation/resend` | Resend confirmation email | Yes | 1/6 min |
| GET | `/auth/email/confirm/<hash>` | Confirm email via recovery hash link | No | 8/minute |
| POST | `/auth/recover` | Initiate password recovery | No | 1/6 min |
| GET | `/auth/confirm/<hash>` | Validate password recovery hash | No | 8/minute |
| POST | `/auth/password` | Set new password (with old password) | Suspended | 10/minute |
| POST | `/auth/reset` | Reset password with recovery hash | No | 8/minute |
| POST | `/auth/account/complete` | Complete user account setup | Yes | No limit |
| GET | `/auth/account/delete` | Initiate account deletion | Yes | No limit |
| POST | `/auth/account/cancel-delete` | Cancel pending account deletion | Yes | No limit |
| GET | `/auth/logout` | Logout user | Yes | No limit |
| GET | `/auth/ip` | Get client IP address | No | No limit |

## OAuth2 Server (`/oauth2`)

Standard OAuth2 endpoints for third-party applications to authenticate with the User Service.

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| GET, POST | `/oauth2/token` | OAuth2 token endpoint | No | No limit |
| GET, POST | `/oauth2/authorize` | OAuth2 authorization endpoint | No | No limit |
| GET | `/oauth2/api/` | List OAuth2 server endpoints | No | No limit |
| GET, POST | `/oauth2/api/me` | Get authenticated user profile via OAuth2 token | Yes | No limit |
| POST | `/oauth2/api/billing` | Get user billing info via OAuth2 token | Yes | No limit |
| GET | `/oauth2/api/email` | Get email endpoints list | No | No limit |

## OAuth2 Client - Social Login (`/provider`)

Connect with external OAuth providers (GitHub, Google, GitLab, etc.).

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| POST | `/provider/login/<provider_name>` | Get OAuth login URL for external provider | No | 15/minute |
| GET | `/provider/authorized/<provider_name>` | OAuth callback handler after external provider auth | No | No limit |
| GET | `/provider/request/<provider_name>/method/<name>/url/<url>` | Make request to external provider API | Yes | No limit |
| POST | `/provider/deauthorized/<provider_name>` | Disconnect OAuth provider account | Yes | No limit |

**Supported Providers**: `gh` (GitHub), `gl` (GitLab), `bb` (Bitbucket), `gc` (Google), `li` (LinkedIn), `azu` (Azure), `aws` (AWS), `do` (DigitalOcean), `lo` (Linode), `fb` (Facebook), `tw` (Twitter)

## Plans & Billing (`/plans`)

Subscription plans, payment processing (Stripe, PayPal), and billing management.

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| POST | `/plans/<payment_method>/<plan_name>` | Subscribe to plan | Yes | No limit |
| GET | `/plans/paypal/change-account` | Change PayPal account | Yes | No limit |
| GET | `/plans/paypal/change-account-test-by-user-id/<user_id>` | Test change PayPal by user ID (admin) | Yes | No limit |
| GET | `/plans/stripe` | Stripe subscription management | No | No limit |
| POST | `/plans/webhook` | Stripe webhook handler | No | No limit |
| POST | `/plans/ipn` | PayPal IPN (Instant Payment Notification) webhook | No | No limit |
| GET | `/plans/info` | Get user plan info and usage | Yes | No limit |
| POST | `/plans/deployment-counter` | Update deployment counter | Yes | No limit |
| GET | `/plans/paypal/process_single_payment` | Process single PayPal payment | Yes | No limit |
| GET | `/plans/paypal/process` | PayPal checkout process | Yes | No limit |
| GET | `/plans/paypal/cancel` | Cancel PayPal checkout | Yes | No limit |

## Email Subscriptions (`/subscriptions`)

Manage user email subscription preferences for newsletters, updates, promotions, etc.

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| GET | `/subscriptions/` | Get all subscription types and user status | Yes | 20/minute |
| POST | `/subscriptions/sub_update` | Update email subscriptions for user | Yes | 20/minute |

**Subscription Update Payload**:
```json
{
  "subscriptions": {
    "promo": "add|remove",
    "updates": "add|remove",
    "newsletter": "add|remove",
    "email_sequences": "add|remove"
  }
}
```

## Installations (`/install`)

Manage stack deployments and installations across cloud providers.

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| GET | `/install/` | List user installations | Yes | No limit |
| GET | `/install/<int:installation_id>` | Get installation details | Yes | No limit |
| POST | `/install/pay/<installation_id>` | Pay for installation | Yes | No limit |
| GET | `/install/start_status_resume/<installation_id>` | Resume installation status check | Yes | No limit |
| POST | `/install/pre-check` | Pre-check installation requirements (cloud provider validation) | Yes | No limit |
| POST | `/install/init/` | Initialize new installation | Yes | No limit |
| GET | `/install/status/<installation_id>` | Get current installation deployment status | Yes | No limit |
| DELETE | `/install/<installation_id>` | Delete installation | Yes | No limit |
| GET | `/install/private/cmd` | Get internal deployment command (internal use) | Yes | No limit |
| GET | `/install/script/<hash_insecure>` | Get key generator script (server registration) | No | No limit |
| GET | `/install/key/<hash_insecure>` | Register server and get deployment key | No | No limit |
| POST | `/install/private/connect` | Private deployment connection endpoint (internal) | No | No limit |

## Migrations (`/migrate`)

Migrate deployments between cloud providers or account transfers.

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| POST | `/migrate/<int:installation_id>/` | Migrate deployment to new cloud provider | Yes | No limit |

## Users Company (`/company`)

Manage company profiles associated with user accounts.

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| GET | `/company/user/<user_id>/company/<company_id>` | Get company for user | Yes | No limit |
| GET | `/company/` | Get authenticated user's company | Yes | No limit |
| POST | `/company/add` | Add new company | Yes | No limit |
| POST | `/company/update` | Update company details | Yes | No limit |
| DELETE | `/company/delete` | Delete company | Yes | No limit |

## Stacks Rating (`/rating`)

User ratings and reviews for stack templates.

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| GET | `/rating/` | Get stack ratings and reviews | Yes | No limit |
| POST | `/rating/add` | Add or update stack rating | Yes | No limit |

## Quick Deploy (`/quick-deploy`)

Quick deployment templates with shareable tokens.

| Method | Endpoint | Description | Auth Required | Rate Limit |
|--------|----------|-------------|----------------|-----------|
| GET | `/quick-deploy/<stack_token>/` | Get quick deploy stack by token | No | No limit |

## Eve REST API (`/api/1.0/<resource>`)

Automatic REST endpoints for database models. Provides full CRUD operations with filtering, sorting, and pagination.

### Available Resources
| Resource | Description | Methods |
|----------|-------------|---------|
| `/api/1.0/users` | User accounts (ACL restricted) | GET, POST, PUT, PATCH, DELETE |
| `/api/1.0/stacks` | Stack templates | GET, POST, PUT, PATCH, DELETE |
| `/api/1.0/apps` | Applications | GET, POST, PUT, PATCH, DELETE |
| `/api/1.0/roles` | User roles and permissions | GET, POST, PUT, PATCH, DELETE |
| `/api/1.0/permissions` | Permission definitions | GET, POST, PUT, PATCH, DELETE |
| `/api/1.0/resources` | ACL resources | GET, POST, PUT, PATCH, DELETE |
| `/api/1.0/stack_view` | Stack marketplace view (read-only) | GET |

See `app/resources.py` for complete list of Eve-managed resources.

### Eve Query Parameters

#### Filtering
```
GET /api/1.0/users?where={"email":"user@example.com"}
```

#### Sorting
```
GET /api/1.0/stacks?sort=[("name", 1)]  # 1 = ascending, -1 = descending
```

#### Pagination
```
GET /api/1.0/stacks?page=1&max_results=50
```

#### ETAG for Updates
Eve requires `If-Match` header with current `_etag` for PUT/PATCH/DELETE:
```
PATCH /api/1.0/users/123
If-Match: "abc123def456"
Content-Type: application/json

{"email": "newemail@example.com"}
```

### Eve Response Format
```json
{
  "_status": "OK",
  "_items": [
    {
      "_id": 1,
      "_etag": "abc123def456",
      "_created": "2025-01-01T12:00:00Z",
      "_updated": "2025-01-02T12:00:00Z",
      "field1": "value1"
    }
  ],
  "_meta": {
    "page": 1,
    "max_results": 50,
    "total": 100
  },
  "_links": {
    "self": {"href": "/api/1.0/resource"},
    "parent": {"href": "/"},
    "next": {"href": "/api/1.0/resource?page=2"}
  }
}
```

## Authentication Methods

### Basic Auth (Eve Resources)
```bash
curl -H "Authorization: Basic base64(email:password)" \
  http://localhost:4100/server/user/api/1.0/users
```

### Bearer Token (OAuth2)
```bash
curl -H "Authorization: Bearer <access_token>" \
  http://localhost:4100/server/user/oauth2/api/me
```

### Session Cookies
Login endpoints set session cookies for browser-based clients:
```bash
curl -b cookies.txt -c cookies.txt -X POST \
  http://localhost:4100/server/user/auth/login \
  -d "email=user@example.com&password=password"
```

### Internal Microservice Auth
Inter-service communication uses bearer token with `INTERNAL_SERVICES_ACCESS_KEY`:
```bash
curl -H "Authorization: Bearer <INTERNAL_SERVICES_ACCESS_KEY>" \
  http://localhost:4100/server/user/api/1.0/users
```

## Error Responses

### Standard Error Format
```json
{
  "_status": "ERR",
  "message": "Error description",
  "code": 400
}
```

### Common HTTP Status Codes
| Code | Meaning |
|------|---------|
| 200 | OK - Request succeeded |
| 201 | Created - Resource created |
| 204 | No Content - Delete successful |
| 400 | Bad Request - Invalid input |
| 401 | Unauthorized - Missing/invalid auth |
| 403 | Forbidden - No permission |
| 404 | Not Found - Resource doesn't exist |
| 409 | Conflict - Duplicate email/resource exists |
| 429 | Too Many Requests - Rate limit exceeded |
| 500 | Internal Server Error |

## Rate Limiting

Rate limits are enforced per client IP address. Responses include headers:
```
X-RateLimit-Limit: 120
X-RateLimit-Remaining: 119
X-RateLimit-Reset: 1234567890
```

If rate limit exceeded:
```json
{
  "_status": "ERR",
  "message": "Rate limit exceeded. Please try again later.",
  "code": 429
}
```

## Payment Methods

### Supported Payment Gateways
- **Stripe** - Credit/debit cards, invoices
- **PayPal** - PayPal account transfers
- **Custom** - Direct payment provider integrations

### Plan Structure
```json
{
  "payment_method": "stripe|paypal",
  "plan_name": "basic|professional|enterprise",
  "billing_cycle": "monthly|yearly",
  "features": {
    "deployments_per_month": 10,
    "storage_gb": 50,
    "team_members": 5
  }
}
```

## Marketplace Integration

The service includes marketplace integration for stack templates:
- **marketplace_template_id** (UUID) - References `stack_template(id)` in Stacker microservice
- **is_from_marketplace** (boolean) - True if stack originated from marketplace
- **template_version** (string) - Version of marketplace template used

Query marketplace stacks:
```bash
GET /api/1.0/stack_view?where={"is_from_marketplace": true}
```

## Webhook Events

Internal AMQP events published via RabbitMQ:
- `workflow.user.register.all` - User registration
- `workflow.user.recover.all` - Password recovery initiated
- `workflow.payment.*` - Payment events (Stripe/PayPal)
- `workflow.install.*` - Installation events
- `workflow.deployment.*` - Deployment status changes
