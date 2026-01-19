<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

# Technical Requirements: TryDirect Marketplace Implementation

**Document Date:** 2025-12-29
**Target:** Backend \& Frontend Development Teams
**Dependencies:** Marketplace schema (`marketplace_schema.sql`) deployed

***

## 1. Core Workflows

### **Workflow 1: Template Creation \& Submission (Stack Builder)**

1. User builds stack in Stack Builder and clicks **"Publish to Marketplace"**
2. System extracts current project configuration as `stack_definition` (JSONB)
3. Frontend presents submission form → calls `POST /api/templates`
4. Backend creates `stack_template` record with `status = 'draft'`
5. User fills metadata → clicks **"Submit for Review"** → `status = 'submitted'`

### **Workflow 2: Admin Moderation**

1. Admin views `/admin/templates?status=submitted`
2. For each template: review `stack_definition`, run security checks
3. Admin approves (`POST /api/admin/templates/{id}/approve`) or rejects with reason
4. On approval: `status = 'approved'`, create `stack_template_review` record

### **Workflow 3: Marketplace Browsing \& Deployment**

1. User visits `/applications` → lists `approved` templates
2. User clicks **"Deploy this stack"** → `GET /api/templates/{slug}`
3. Frontend loads latest `stack_template_version.stack_definition` into Stack Builder
4. New `project` created with `source_template_id` populated
5. User customizes and deploys normally

### **Workflow 4: Paid Template Purchase**

1. User selects paid template → redirected to Stripe checkout
2. On success: create `template_purchase` record
3. Unlock access → allow deployment

***

## 2. Backend API Specifications

### **Public Endpoints (no auth)**

```
GET  /api/templates              # List approved templates (paginated)
     ?category=AI+Agents&tag=n8n&sort=popular
GET  /api/templates/{slug}        # Single template details + latest version
```

**Response Structure:**

```
{
  "id": "uuid",
  "slug": "ai-agent-starter",
  "name": "AI Agent Starter Stack",
  "short_description": "...",
  "long_description": "...",
  "status": "approved",
  "creator": {"id": "user-123", "name": "Alice Dev"},
  "category": {"id": 1, "name": "AI Agents"},
  "tags": ["ai", "n8n", "qdrant"],
  "tech_stack": {"services": ["n8n", "Qdrant"]},
  "stats": {
    "deploy_count": 142,
    "average_rating": 4.7,
    "view_count": 2500
  },
  "pricing": {
    "plan_type": "free",
    "price": null
  },
  "latest_version": {
    "version": "1.0.2",
    "stack_definition": {...}  // Full YAML/JSON
  }
}
```


### **Authenticated Creator Endpoints**

```
POST /api/templates              # Create draft from current project
PUT  /api/templates/{id}         # Edit metadata (only draft/rejected)
POST /api/templates/{id}/submit  # Submit for review
GET  /api/templates/mine         # User's templates + status
```


### **Admin Endpoints**

```
GET  /api/admin/templates?status=submitted  # Pending review
POST /api/admin/templates/{id}/approve      # Approve template
POST /api/admin/templates/{id}/reject       # Reject with reason
```


***

## 3. Frontend Integration Points

### **Stack Builder (Project Detail Page)**

**New Panel: "Publish to Marketplace"**

```
[ ] I confirm this stack contains no secrets/API keys

📝 Name: [AI Agent Starter Stack]
🏷️  Category: [AI Agents ▼]
🔖 Tags: [n8n] [qdrant] [ollama] [+ Add tag]
📄 Short Description: [Deploy production-ready...]
💰 Pricing: [Free ○] [One-time $29 ●] [Subscription $9/mo ○]

Status: [Not submitted] [In review] [Approved! View listing]
[Submit for Review] [Edit Draft]
```


### **Applications Page (`/applications`)**

**Template Card Structure:**

```
[Icon] AI Agent Starter Stack
"Deploy n8n + Qdrant + Ollama in 5 minutes"
⭐ 4.7 (28)  🚀 142 deploys  👀 2.5k views
By Alice Dev  •  AI Agents  •  n8n qdrant ollama
[Free] [Deploy this stack] [View details]
```


### **Admin Dashboard**

**Template Review Interface:**

```
Template: AI Agent Starter Stack v1.0.0
Status: Submitted 2h ago
Creator: Alice Dev

[View Stack Definition] [Security Scan] [Test Deploy]

Security Checklist:
☐ No secrets detected
☐ Valid Docker syntax
☐ No malicious code
[Notes] [Approve] [Reject] [Request Changes]
```


***

## 4. Data Structures \& Field Constraints

### **`stack_template` Table**

| Field | Type | Constraints | Description |
| :-- | :-- | :-- | :-- |
| `id` | UUID | PK | Auto-generated |
| `creator_user_id` | VARCHAR(50) | FK `users(id)` | Template owner |
| `name` | VARCHAR(255) | NOT NULL | Display name |
| `slug` | VARCHAR(255) | UNIQUE | URL: `/applications/{slug}` |
| `status` | VARCHAR(50) | CHECK: draft\|submitted\|... | Lifecycle state |
| `plan_type` | VARCHAR(50) | CHECK: free\|one_time\|subscription | Pricing model |
| `tags` | JSONB | DEFAULT `[]` | `["n8n", "qdrant"]` |

### **`stack_template_version` Table**

| Field | Type | Constraints | Description |
| :-- | :-- | :-- | :-- |
| `template_id` | UUID | FK | Links to template |
| `version` | VARCHAR(20) | UNIQUE w/ template_id | Semver: "1.0.2" |
| `stack_definition` | JSONB | NOT NULL | Docker Compose YAML as JSON |
| `is_latest` | BOOLEAN | DEFAULT false | Only one true per template |

### **Status Value Constraints**

```
stack_template.status: ['draft', 'submitted', 'under_review', 'approved', 'rejected', 'deprecated']
stack_template_review.decision: ['pending', 'approved', 'rejected', 'needs_changes']
stack_template.plan_type: ['free', 'one_time', 'subscription']
```


***

## 5. Security \& Validation Requirements

### **Template Submission Validation**

1. **Secret Scanning**: Regex check for API keys, passwords in `stack_definition`
2. **Docker Syntax**: Parse YAML, validate service names/ports/volumes
3. **Resource Limits**: Reject templates requiring >64GB RAM
4. **Malware Scan**: Check docker images against vulnerability DB

### **Review Checklist Fields** (`security_checklist` JSONB)

```
{
  "no_secrets": true,
  "no_hardcoded_creds": true,
  "valid_docker_syntax": true,
  "no_malicious_code": true,
  "reasonable_resources": true
}
```


### **Casbin Permissions** (extend existing rules)

```
# Creators manage their templates
p, creator_user_id, stack_template, edit, template_id
p, creator_user_id, stack_template, delete, template_id

# Admins review/approve
p, admin, stack_template, approve, *
p, admin, stack_template_review, create, *

# Public read approved templates
p, *, stack_template, read, status=approved
```


***

## 6. Analytics \& Metrics

### **Template Stats (updated via triggers)**

- `deploy_count`: Count `project` records with `source_template_id`
- `average_rating`: AVG from `stack_template_rating`
- `view_count`: Increment on `GET /api/templates/{slug}`


### **Creator Dashboard Metrics**

```
Your Templates (3)
• AI Agent Stack: 142 deploys, $1,240 earned
• RAG Pipeline: 28 deploys, $420 earned
• Data ETL: 5 deploys, $0 earned (free)

Total Revenue: $1,660 (80% share)
```


***

## 7. Integration Testing Checklist

- [ ] User can submit template from Stack Builder → appears in admin queue
- [ ] Admin approves template → visible on `/applications`
- [ ] User deploys template → `project.source_template_id` populated
- [ ] Stats update correctly (views, deploys, ratings)
- [ ] Paid template purchase → deployment unlocked
- [ ] Rejected template → creator receives reason, can resubmit

***

## 8. Deployment Phases

**Week 1:** Backend tables + core APIs (`stack_template`, review workflow)
**Week 2:** Frontend integration (Stack Builder panel, `/applications` cards)
**Week 3:** Monetization (Stripe, `template_purchase`)
**Week 4:** Admin dashboard + analytics

This spec provides complete end-to-end implementation guidance without code examples.
<span style="display:none">[^1][^2][^3]</span>

<div align="center">⁂</div>

[^1]: https://ppl-ai-file-upload.s3.amazonaws.com/web/direct-files/attachments/images/156249360/1badb17d-ae6d-4002-b9c0-9371e2a0cdb9/Screenshot-2025-12-28-at-21.25.20.jpg

[^2]: https://ppl-ai-file-upload.s3.amazonaws.com/web/direct-files/attachments/156249360/821876d8-35e0-46f9-af9c-b318f416d680/dump-stacker-202512291130.sql

[^3]: https://ppl-ai-file-upload.s3.amazonaws.com/web/direct-files/attachments/156249360/9cbd962c-d7b5-40f6-a86d-8a05280502ed/TryDirect-DB-diagram.graphml

