# Quick Reference: Open Questions Resolutions

**Status**: ✅ Research Complete | 🔄 Awaiting Team Confirmation  
**Date**: 9 January 2026  
**Full Details**: See [OPEN_QUESTIONS_RESOLUTIONS.md](OPEN_QUESTIONS_RESOLUTIONS.md)

---

## The 4 Questions & Proposed Answers

### 1️⃣ Health Check Contract
```
URL: GET /api/health/deployment/{deployment_hash}/app/{app_code}
Timeout: 10 seconds
Status Codes: 200 (healthy) | 202 (degraded) | 503 (unhealthy)

Response: {
  "status": "healthy|degraded|unhealthy",
  "timestamp": "2026-01-09T12:00:00Z",
  "deployment_hash": "abc123",
  "app_code": "nginx",
  "details": { "response_time_ms": 42, "checks": [...] }
}
```

### 2️⃣ Rate Limits
```
Deploy endpoint:    10 requests/min
Restart endpoint:   5 requests/min
Logs endpoint:     20 requests/min
Status endpoint:   60 requests/min

Plan Tiers:
- Free:       5 deployments/hour
- Plus:      20 deployments/hour
- Enterprise: 100 deployments/hour

Implementation: Redis-backed per-user limits (not IP-based)
```

### 3️⃣ Log Redaction
```
Patterns Redacted:
1. Environment variables (API_KEY=..., PASSWORD=...)
2. AWS credentials (AKIAIOSFODNN...)
3. API tokens (Bearer ..., Basic ...)
4. PII (email addresses)
5. Credit cards (4111-2222-3333-4444)
6. SSH private keys

20 Env Vars Blacklisted:
AWS_SECRET_ACCESS_KEY, DATABASE_URL, DB_PASSWORD, PGPASSWORD,
API_KEY, API_SECRET, SECRET_KEY, STRIPE_SECRET_KEY,
GITHUB_TOKEN, GITLAB_TOKEN, SENDGRID_API_KEY, ...

Implementation: Regex patterns applied before log return
```

### 4️⃣ Container→App Code Mapping
```
Canonical Source: app_code (from Stacker project.metadata)

Data Flow:
  Stacker deploys
    ↓
  sends project.metadata.apps[].app_code to User Service
    ↓
  User Service stores in deployment_apps table
    ↓
  Status Panel queries deployment_apps for app list
    ↓
  Status Panel maps app_code → container_name for UI

User Service Table:
CREATE TABLE deployment_apps (
  id UUID,
  deployment_hash VARCHAR(64),
  installation_id INTEGER,
  app_code VARCHAR(255),           ← Canonical
  container_name VARCHAR(255),
  image VARCHAR(255),
  ports JSONB,
  metadata JSONB
)
```

---

## Implementation Roadmap

| Phase | Task | Hours | Priority |
|-------|------|-------|----------|
| 1 | Health Check Endpoint | 6-7h | 🔴 HIGH |
| 2 | Rate Limiter Middleware | 6-7h | 🔴 HIGH |
| 3 | Log Redaction Service | 5h | 🟡 MEDIUM |
| 4 | User Service Schema | 3-4h | 🔴 HIGH |
| 5 | Integration Tests | 6-7h | 🟡 MEDIUM |
| 6 | Documentation | 4-5h | 🟢 LOW |
| **Total** | | **30-35h** | — |

---

## Status Panel Command Payloads

- **Canonical schemas** now live in `src/forms/status_panel.rs`; Rust validation covers both command creation and agent reports.
- Health, logs, and restart payloads require `deployment_hash` + `app_code` plus the fields listed in [AGENT_REGISTRATION_SPEC.md](AGENT_REGISTRATION_SPEC.md#field-reference-canonical-schemas).
- Agents must return structured reports (metrics/log lines/restart status). Stacker rejects malformed responses before persisting to `commands`.
- All requests remain signed with the Vault-fetched agent token (HMAC headers) as documented in `STACKER_INTEGRATION_REQUIREMENTS.md`.

---

## Files Created

✅ [OPEN_QUESTIONS_RESOLUTIONS.md](OPEN_QUESTIONS_RESOLUTIONS.md) - Full proposal document (500+ lines)  
✅ [OPEN_QUESTIONS_SUMMARY.md](OPEN_QUESTIONS_SUMMARY.md) - Executive summary  
✅ [IMPLEMENTATION_ROADMAP.md](IMPLEMENTATION_ROADMAP.md) - Task breakdown (22 tasks)  
✅ [TODO.md](../TODO.md) - Updated with status and links (lines 8-21)  
✅ `/memories/open_questions.md` - Internal tracking  

---

## For Quick Review

**Want just the answers?** → Read this file  
**Want full proposals with rationale?** → Read [OPEN_QUESTIONS_RESOLUTIONS.md](OPEN_QUESTIONS_RESOLUTIONS.md)  
**Want to start implementation?** → Read [IMPLEMENTATION_ROADMAP.md](IMPLEMENTATION_ROADMAP.md)  
**Want to track progress?** → Check `/memories/open_questions.md`  

---

## Checklist for Team

- [ ] Review proposed answers (this file or full document)
- [ ] Confirm health check endpoint design
- [ ] Confirm rate limit thresholds
- [ ] Confirm log redaction patterns
- [ ] Confirm User Service schema changes
- [ ] Coordinate with User Service team on deployment_apps table
- [ ] Coordinate with Status Panel team on health check consumption
- [ ] Assign tasks to engineers
- [ ] Update sprint/roadmap
- [ ] Begin Phase 1 implementation

---

## Key Decisions

✅ **Why REST health check vs webhook?**  
→ Async polling is simpler and more reliable; no callback server needed in Status Panel

✅ **Why Redis rate limiting?**  
→ Per-user (not IP) limits work for internal services; shared state across instances

✅ **Why regex-based log redaction?**  
→ Whitelist approach catches known patterns; safer than blacklist for security

✅ **Why deployment_apps table?**  
→ Fast O(1) lookups for Status Panel; avoids JSON parsing; future-proof schema

---

## Questions? Next Steps?

1. **Feedback on proposals?** → Update TODO.md or OPEN_QUESTIONS_RESOLUTIONS.md
2. **Need more details?** → Open [OPEN_QUESTIONS_RESOLUTIONS.md](OPEN_QUESTIONS_RESOLUTIONS.md)
3. **Ready to implement?** → Open [IMPLEMENTATION_ROADMAP.md](IMPLEMENTATION_ROADMAP.md)
4. **Tracking progress?** → Update `/memories/open_questions.md`

---

**Status**: ✅ Research Complete  
**Next**: Await team confirmation → Begin implementation → Track progress

Last updated: 2026-01-09
