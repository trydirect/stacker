# Vendor Profile Endpoints Spec

Frontend-facing API reference for vendor profile and vendor page flows.

## Status

- Public vendor page endpoint: implemented
- Creator vendor profile endpoints: implemented
- Admin vendor profile endpoint: implemented
- Creator onboarding flow: provider-backed via `PayoutProvider`
  - dev/test default: `mock`
  - production option: `stripe_connect`

---

## Base conventions

### Auth

- Public vendor page endpoint does **not** require authentication
- Creator endpoints require `Authorization: Bearer <token>`
- Admin endpoints require an admin-authenticated bearer token

### Response envelope

Most JSON endpoints use this envelope:

```json
{
  "message": "OK",
  "item": {}
}
```

Some endpoints return only a message:

```json
{
  "message": "Vendor profile updated"
}
```

### Common vendor concepts

#### `verification_status`
Allowed values:
- `unverified`
- `pending`
- `verified`
- `rejected`

#### `onboarding_status`
Allowed values:
- `not_started`
- `in_progress`
- `completed`

#### `payout_ready`
Computed server-side and should be treated as the canonical frontend flag.

`payout_ready = true` only when:
- `verification_status == "verified"`
- `onboarding_status == "completed"`
- `payouts_enabled == true`
- `payout_provider != null`

---

# 1. Public vendor page

## GET `/api/vendors/{vendor}`

Returns the public vendor profile and the vendor's approved marketplace templates.

### Path param

| Name | Type | Required | Notes |
|---|---|---:|---|
| `vendor` | string | yes | Public vendor slug is preferred. Current backend also supports fallback lookup by `creator_user_id`, but frontend should use the slug. |

### Query params

| Name | Type | Required | Allowed |
|---|---|---:|---|
| `sort` | string | no | `recent`, `popular`, `rating` |

### Auth

None.

### Success response

```json
{
  "message": "OK",
  "item": {
    "vendor": {
      "creator_user_id": "vendor_acme",
      "slug": "acme-cloud",
      "display_name": "Acme Cloud",
      "bio": "Production-ready app stacks for small teams.",
      "avatar_url": "https://cdn.trydirect.test/vendors/acme/avatar.png",
      "website_url": "https://acme.example",
      "verified": true,
      "rating": 4.0,
      "rating_count": 3,
      "rating_scale": 5,
      "metadata": {
        "country": "DE",
        "support_email": "support@acme.example"
      },
      "created_at": "2026-07-18T12:00:00Z"
    },
    "templates": [
      {
        "id": "3ec49835-3950-44e2-b289-0d725d310902",
        "creator_user_id": "vendor_acme",
        "creator_name": "Acme Cloud",
        "name": "WordPress Pro",
        "slug": "wordpress-pro",
        "short_description": "Production WordPress stack",
        "long_description": "A hardened WordPress stack for teams.",
        "status": "approved",
        "vendor_url": "https://acme.example",
        "tags": ["wordpress", "cms"],
        "tech_stack": {
          "runtime": "php",
          "database": "mysql"
        },
        "view_count": 0,
        "deploy_count": 0,
        "created_at": "2026-07-18T12:00:00Z",
        "updated_at": "2026-07-18T12:00:00Z",
        "approved_at": "2026-07-18T12:00:00Z",
        "verifications": {},
        "infrastructure_requirements": {},
        "public_ports": null,
        "required_plan_name": null,
        "price": null,
        "billing_cycle": null,
        "currency": null,
        "rating": 4.5,
        "rating_count": 2,
        "rating_scale": 5
      }
    ]
  }
}
```

### Notes

- Only **approved** templates are returned.
- Draft/submitted/rejected templates are excluded.
- Sensitive payout/account linkage fields are **not** exposed publicly.
- Public vendor metadata excludes the internal `onboarding` metadata subtree.
- Public `rating` values are normalized to a 1-5 star scale and use only visible `Application` ratings.
- `rating_count` is the number of visible `Application` ratings behind the average.

### Error responses

#### 404 Vendor not found

```json
{
  "message": "Vendor not found"
}
```

### Frontend usage

Use this endpoint to build a public vendor page:
- vendor header
- avatar/logo
- bio/about section
- website link
- vendor template grid/list

---

# 2. Template ratings

These endpoints are the preferred way for frontend code to rate marketplace templates. They hide the internal `product_id` / `rating.obj_id` mapping.

## GET `/api/templates/{templateId}/rating/summary`

Returns public rating summary for an approved template.

### Auth

None.

### Success response

```json
{
  "message": "OK",
  "item": {
    "template_id": "3ec49835-3950-44e2-b289-0d725d310902",
    "rating": 4.5,
    "rating_count": 2,
    "rating_scale": 5
  }
}
```

## GET `/api/templates/{templateId}/rating/me`

Returns the authenticated user's own visible rating for a template.

### Auth

Bearer token required.

### Success response

```json
{
  "message": "OK",
  "item": {
    "template_id": "3ec49835-3950-44e2-b289-0d725d310902",
    "rating_id": 42,
    "rating": 5,
    "rating_scale": 5,
    "comment": "Excellent marketplace template",
    "created_at": "2026-07-18T12:00:00Z",
    "updated_at": "2026-07-18T12:00:00Z"
  }
}
```

## PUT `/api/templates/{templateId}/rating`

Creates or updates the authenticated user's rating for a template.

### Auth

Bearer token required.

### Request body

```json
{
  "rating": 5,
  "comment": "Excellent marketplace template"
}
```

### Validation

- `rating` is required and must be an integer from `1` to `5`.
- `comment` is optional and must be at most 1000 characters.

### Success response

Same shape as `GET /api/templates/{templateId}/rating/me`.

## DELETE `/api/templates/{templateId}/rating`

Deletes the authenticated user's template rating from public averages by hiding it.

### Auth

Bearer token required.

### Success response

```json
{
  "message": "Rating deleted"
}
```

### Notes

- Frontend should use these template rating endpoints instead of generic `/rating` for marketplace template pages.
- The backend resolves `templateId -> product_id -> rating.obj_id` internally.
- Internally ratings are still stored on a 0-10 scale for compatibility; these endpoints expose a 1-5 star scale.
- Public vendor/template averages use visible `Application` ratings only.

---

# 3. Creator self vendor profile

## GET `/api/templates/mine/vendor-profile`

Returns the authenticated creator's own vendor profile.

### Auth

Bearer token required.

### Success response

```json
{
  "message": "OK",
  "item": {
    "creator_user_id": "user_123",
    "payout_ready": false,
    "vendor_profile": {
      "creator_user_id": "user_123",
      "verification_status": "unverified",
      "onboarding_status": "not_started",
      "payouts_enabled": false,
      "payout_provider": null,
      "metadata": {},
      "created_at": null,
      "updated_at": null
    }
  }
}
```

### Notes

- If no DB row exists yet, the API returns a safe default profile.
- `payout_account_ref` is intentionally not exposed in this creator-facing response.

### Error responses

#### 403 Authentication required

```json
{
  "message": "Authentication required"
}
```

---

# 3. Creator template-scoped vendor profile status

## GET `/api/templates/{templateId}/vendor-profile-status`

Returns vendor profile status for the owner of a specific template.

### Path param

| Name | Type | Required |
|---|---|---:|
| `templateId` | UUID string | yes |

### Auth

Bearer token required.

### Access rule

Caller must own the template.

### Success response

```json
{
  "message": "OK",
  "item": {
    "template_id": "3ec49835-3950-44e2-b289-0d725d310902",
    "creator_user_id": "user_123",
    "payout_ready": true,
    "vendor_profile": {
      "creator_user_id": "user_123",
      "verification_status": "verified",
      "onboarding_status": "completed",
      "payouts_enabled": true,
      "payout_provider": "stripe_connect",
      "metadata": {
        "country": "DE"
      },
      "created_at": "2026-04-12T12:00:00Z",
      "updated_at": "2026-04-12T12:05:00Z"
    }
  }
}
```

### Error responses

#### 400 Invalid UUID

```json
{
  "message": "Invalid UUID"
}
```

#### 403 Authentication required

```json
{
  "message": "Authentication required"
}
```

#### 403 Access denied

```json
{
  "message": "Access denied"
}
```

#### 404 Template not found

```json
{
  "message": "Template not found"
}
```

---

# 4. Start creator onboarding

## POST `/api/templates/mine/vendor-profile/onboarding-link`

Starts or reuses a creator onboarding linkage.

### Auth

Bearer token required.

### Request body

No request body required.

### Success response

```json
{
  "message": "OK",
  "item": {
    "creator_user_id": "user_123",
    "payout_ready": false,
    "linkage_created": true,
    "onboarding_url": "https://mock.payouts.local/onboarding/acct_mock_123",
    "onboarding_expires_at": null,
    "vendor_profile": {
      "creator_user_id": "user_123",
      "verification_status": "unverified",
      "onboarding_status": "in_progress",
      "payouts_enabled": false,
      "payout_provider": "mock",
      "metadata": {
        "onboarding": {
          "started_at": "2026-04-12T12:00:00Z",
          "last_link_requested_at": "2026-04-12T12:00:00Z",
          "link_request_count": 1
        }
      },
      "created_at": "2026-04-12T12:00:00Z",
      "updated_at": "2026-04-12T12:00:00Z"
    }
  }
}
```

### Notes

- Provider is selected by backend config (`payouts.provider`).
- Dev/test default uses `payout_provider = "mock"`.
- Production can use `payout_provider = "stripe_connect"` with Stripe Connect account links.
- Repeated calls can return `200 OK` with `linkage_created = false` when linkage already exists.

### Error responses

#### 403 Authentication required

```json
{
  "message": "Authentication required"
}
```

---

# 5. Complete creator onboarding

## POST `/api/templates/mine/vendor-profile/onboarding-complete`

Marks onboarding as completed for the authenticated creator.

### Auth

Bearer token required.

### Request body

No request body required.

### Success response

```json
{
  "message": "OK",
  "item": {
    "creator_user_id": "user_123",
    "payout_ready": false,
    "completion_recorded": true,
    "vendor_profile": {
      "creator_user_id": "user_123",
      "verification_status": "pending",
      "onboarding_status": "completed",
      "payouts_enabled": false,
      "payout_provider": "mock",
      "metadata": {
        "onboarding": {
          "started_at": "2026-04-12T12:00:00Z",
          "completed_at": "2026-04-12T12:05:00Z",
          "completion_source": "creator_api"
        }
      },
      "created_at": "2026-04-12T12:00:00Z",
      "updated_at": "2026-04-12T12:05:00Z"
    }
  }
}
```

### Behavior

- Idempotent.
- If onboarding was already completed, the request still returns `200 OK` and `completion_recorded = false`.
- If onboarding was never started, the endpoint returns `409`.

### Error responses

#### 403 Authentication required

```json
{
  "message": "Authentication required"
}
```

#### 409 Onboarding link must exist before completion

```json
{
  "message": "Onboarding link must exist before completion"
}
```

---

# 6. Payout provider webhook

## POST `/api/v1/marketplace/payouts/webhook`

Receives payout-provider webhook events. Currently implemented for Stripe Connect `account.updated` events.

### Auth

No bearer token. The endpoint is public for provider delivery, but Stripe payloads are verified with `Stripe-Signature` when `STRIPE_WEBHOOK_SECRET` / `PAYOUT_STRIPE_WEBHOOK_SECRET` is configured.

### Headers

```http
Stripe-Signature: t=...,v1=...
```

### Behavior

For Stripe Connect:

- Ignores unsupported event types.
- Handles `account.updated`.
- Reads `data.object.id` as the connected account id.
- Reads `details_submitted` as onboarding completion state.
- Reads `payouts_enabled` as provider payout capability.
- Updates the matching vendor profile by `payout_provider = "stripe_connect"` and `payout_account_ref = account.id`.

### Success responses

```json
{
  "message": "Webhook processed"
}
```

or for ignored event types:

```json
{
  "message": "Webhook ignored"
}
```

### Error responses

Invalid signature / invalid payload returns `400`.

---

# 7. Admin update vendor profile

## PATCH `/api/admin/templates/{templateId}/vendor-profile`

Admin endpoint to create or update the template creator's vendor profile.

### Path param

| Name | Type | Required |
|---|---|---:|
| `templateId` | UUID string | yes |

### Auth

Admin bearer token required.

### Request body

All fields are optional, but at least one field must be provided.

```json
{
  "verification_status": "pending",
  "onboarding_status": "in_progress",
  "payouts_enabled": false,
  "payout_provider": "stripe_connect",
  "payout_account_ref": "acct_123",
  "metadata": {
    "country": "NL"
  }
}
```

### Validation

- body must not be empty
- `metadata` must be a JSON object
- `verification_status` must be one of:
  - `unverified`
  - `pending`
  - `verified`
  - `rejected`
- `onboarding_status` must be one of:
  - `not_started`
  - `in_progress`
  - `completed`

### Success response

```json
{
  "message": "Vendor profile updated"
}
```

### Error responses

#### 400 Invalid status or invalid body

```json
{
  "message": "Invalid verification_status 'not-a-real-status'. Allowed values: unverified, pending, verified, rejected"
}
```

```json
{
  "message": "No vendor profile fields provided"
}
```

```json
{
  "message": "metadata must be a JSON object"
}
```

#### 404 Template not found

```json
{
  "message": "Template not found"
}
```

#### 403 Forbidden

Admin auth required.

---

# Public vendor object shape

Used in `GET /api/vendors/{vendor}`.

```ts
export interface PublicVendorProfile {
  creator_user_id: string;
  slug: string | null;
  display_name: string | null;
  bio: string | null;
  avatar_url: string | null;
  website_url: string | null;
  verified: boolean;
  rating: number | null;
  rating_count: number;
  rating_scale: 5;
  metadata: Record<string, unknown>;
  created_at: string | null;
}

export interface PublicVendorTemplate {
  id: string;
  creator_user_id: string;
  creator_name: string | null;
  name: string;
  slug: string;
  short_description: string | null;
  long_description: string | null;
  status: string;
  vendor_url: string | null;
  tags: unknown;
  tech_stack: unknown;
  view_count: number | null;
  deploy_count: number | null;
  created_at: string | null;
  updated_at: string | null;
  approved_at: string | null;
  rating: number | null;
  rating_count: number;
  rating_scale: 5;
}

export interface TemplateRatingSummary {
  template_id: string;
  rating: number | null;
  rating_count: number;
  rating_scale: 5;
}

export interface MyTemplateRating {
  template_id: string;
  rating_id: number;
  rating: number;
  rating_scale: 5;
  comment: string | null;
  created_at: string;
  updated_at: string;
}
```

---

# Creator vendor profile object shape

Used in creator self/status/onboarding responses.

```ts
export interface VendorProfile {
  creator_user_id: string;
  verification_status: 'unverified' | 'pending' | 'verified' | 'rejected';
  onboarding_status: 'not_started' | 'in_progress' | 'completed';
  payouts_enabled: boolean;
  payout_provider: string | null;
  metadata: Record<string, unknown>;
  created_at: string | null;
  updated_at: string | null;
}
```

---

# Frontend implementation notes

## Public vendor page

Recommended flow:
1. route user to `/vendors/:slug`
2. call `GET /api/vendors/:slug`
3. render:
   - avatar/logo
   - display name
   - bio
   - website link
   - approved template cards

## Creator dashboard

Recommended flow:
1. call `GET /api/templates/mine/vendor-profile`
2. if onboarding has not started, call `POST /api/templates/mine/vendor-profile/onboarding-link`
3. once onboarding is complete, call `POST /api/templates/mine/vendor-profile/onboarding-complete`
4. use `payout_ready` as the main CTA state signal

## Payout provider configuration

The onboarding endpoints are backed by a provider abstraction.

- `mock` is the default for dev/test and returns a mock onboarding URL.
- `stripe_connect` creates/reuses Stripe Express accounts and returns Stripe account onboarding links.

Runtime config:

```yaml
payouts:
  provider: mock # or stripe_connect
  stripe_api_base_url: https://api.stripe.com
  onboarding_return_url: https://stacker.try.direct/marketplace/vendor/onboarding/return
  onboarding_refresh_url: https://stacker.try.direct/marketplace/vendor/onboarding/refresh
  timeout_secs: 15
```

Secrets must come from environment variables:

```bash
STACKER_PAYOUT_PROVIDER=stripe_connect
STRIPE_SECRET_KEY=sk_live_...
STRIPE_WEBHOOK_SECRET=whsec_...
```

Frontend should use the returned `onboarding_url` rather than assuming a provider-specific URL format.

Stripe webhook target:

```text
POST https://<stacker-public-host>/api/v1/marketplace/payouts/webhook
```
