<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

## Updated: Cross-Microservice Integration for `/applications`

**Key Challenge:** `/applications` endpoint lives in a **separate microservice** (TryDirect User Service) (not Stacker). Marketplace templates must be **federated** into this external catalog.

***

## **1. New Microservice Communication Pattern**

### **Option A: API Federation (Recommended)**

Stacker Marketplace → **publishes approved templates** to TryDirect User microservice via **webhook/API**.

```
Approved Template in Stacker
         ↓
POST /api/stack/templates  ← Stacker webhook
         ↓
TryDirect User microservice stores in OWN `marketplace_templates` table
         ↓
Unified /applications endpoint serves both official + marketplace
```


### **Option B: Query Federation**

User service microservice **queries Stacker** for approved templates on each request.

```
GET /applications
  ↓
User service microservice:
  - Official stacks (local DB)
  + Marketplace templates (GET Stacker /api/templates?status=approved)
  ↓
Unified response
```

**Recommendation: Option A** (webhook) – better performance, caching, unified data model.

***

## **2. Stacker → TryDirect User Microservice Webhook Flow**

### **When template approved in Stacker:**

```
1. Admin approves → stack_template.status = 'approved'
2. Stacker fires webhook:
   POST https://user:4100/marketplace/sync
   
   Body:
   {
     "action": "template_approved",
     "template_id": "uuid-123",
     "slug": "ai-agent-starter",
     "stack_definition": {...},
     "creator": "Alice Dev",
     "stats": {"deploy_count": 0}
   }
3. TryDirect User service creates/updates ITS local copy
```


### **When template updated/rejected/deprecated:**

```
Same webhook with action: "template_updated", "template_rejected", "template_deprecated"
```


***

## **3. TryDirect User Microservice Requirements**

**Add to TryDirect User service (not Stacker):**

### **New Table: `marketplace_templates`**

```
id UUID PK
stacker_template_id UUID  ← Links back to Stacker
slug VARCHAR(255) UNIQUE
name VARCHAR(255)
short_description TEXT
creator_name VARCHAR(255)
category VARCHAR(100)
tags JSONB
pricing JSONB
stats JSONB  ← {deploy_count, rating, views}
stack_definition JSONB  ← Cached for fast loading
is_active BOOLEAN DEFAULT true
synced_at TIMESTAMP
```


### **New Endpoint: `/api/marketplace/sync` (TryDirect User service)**

```
POST /api/marketplace/sync
Headers: Authorization: Bearer stacker-service-token

Actions:
- "template_approved" → INSERT/UPDATE marketplace_templates
- "template_updated" → UPDATE marketplace_templates  
- "template_rejected" → SET is_active = false
- "template_deprecated" → DELETE
```


### **Updated `/applications` Query (TryDirect User service):**

```sql
-- Official stacks (existing)
SELECT * FROM stacks WHERE is_active = true

UNION ALL

-- Marketplace templates (new table)
SELECT 
  id, name, slug,
  short_description as description,
  creator_name,
  '👥 Community' as badge,
  stats->>'deploy_count' as deploy_count
FROM marketplace_templates 
WHERE is_active = true
ORDER BY popularity DESC
```


***

## **4. Stack Builder Integration Changes (Minimal)**

Stacker only needs to:

1. **Add marketplace tables** (as per schema)
2. **Implement webhook client** on template status changes
3. **Expose public API** for TryDirect User service:

```
GET /api/templates?status=approved  ← For fallback/sync
GET /api/templates/{slug}           ← Stack definition + stats
```


**Stack Builder UI unchanged** – "Publish to Marketplace" still works the same.

***

## **5. Service-to-Service Authentication**

### **Webhook Security:**

```
Stack → TryDirect User:
- API Token: `stacker_service_token` (stored in TryDirect User env)
- Verify `stacker_service_token` header matches expected value
- Rate limit: 100 req/min
```


### **Fallback Query Security (if webhook fails):**

```
TryDirect User → Stacker:
- API Key: `applications_service_key` (stored in Stacker env)
- Stacker verifies key on `/api/templates` endpoints
```


***

## **6. Deployment Coordination**

### **Phase 1: Stacker Changes**

```
✅ Deploy marketplace_schema.sql
✅ Implement template APIs + webhook client
✅ Test "template approved → webhook fires"
```


### **Phase 2: TryDirect User Service Changes**

```
✅ Add marketplace_templates table
✅ Implement /api/marketplace/sync webhook receiver
✅ Update /applications endpoint (UNION query)
✅ Test webhook → unified listing
```


### **Phase 3: Stack Builder UI**

```
✅ "Publish to Marketplace" panel
✅ Template cards show on /applications
✅ "Deploy this stack" → loads from TryDirect User cache
```


***

## **7. Fallback \& Resilience**

**If webhook fails:**

```
1. TryDirect User service queries Stacker directly (every 15min cron)
2. Mark templates as "stale" if >1h out of sync
3. Show warning badge: "🔄 Syncing..."
```

**Data Consistency:**

```
Stacker = Source of Truth (approved templates)
TryDirect User = Cache (fast listing + stack_definitions)
```


***

## **Summary: Clean Microservice Boundaries**

```
Stacker responsibilities:
├── Marketplace tables + workflows
├── Template submission/review
└── Webhook: "template approved → notify TryDirect User"

TryDirect User responsibilities:
├── Unified /applications listing
├── marketplace_templates cache table
├── Webhook receiver /api/marketplace/sync
└── "Deploy this stack" → return cached stack_definition
```

**Result:** Zero changes to existing `/applications` consumer code. Marketplace templates appear **naturally** alongside official stacks. 🚀
<span style="display:none">[^1][^2][^3]</span>

<div align="center">⁂</div>

[^1]: https://ppl-ai-file-upload.s3.amazonaws.com/web/direct-files/attachments/images/156249360/1badb17d-ae6d-4002-b9c0-9371e2a0cdb9/Screenshot-2025-12-28-at-21.25.20.jpg

[^2]: https://ppl-ai-file-upload.s3.amazonaws.com/web/direct-files/attachments/156249360/821876d8-35e0-46f9-af9c-b318f416d680/dump-stacker-202512291130.sql

[^3]: https://ppl-ai-file-upload.s3.amazonaws.com/web/direct-files/attachments/156249360/9cbd962c-d7b5-40f6-a86d-8a05280502ed/TryDirect-DB-diagram.graphml

