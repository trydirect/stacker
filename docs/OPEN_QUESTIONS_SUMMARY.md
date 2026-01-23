# Status Panel & MCP Integration - Resolution Summary

**Date**: 9 January 2026  
**Status**: ✅ RESEARCH COMPLETE - AWAITING TEAM CONFIRMATION  

---

## Executive Summary

All four open questions from [TODO.md](../TODO.md#new-open-questions-status-panel--mcp) have been researched and comprehensive proposals have been documented in **[docs/OPEN_QUESTIONS_RESOLUTIONS.md](OPEN_QUESTIONS_RESOLUTIONS.md)**.

---

## Quick Reference

### Question 1: Health Check Contract
**Proposed**: `GET /api/health/deployment/{deployment_hash}/app/{app_code}`
- Status codes: 200 (healthy), 202 (degraded), 503 (unhealthy)
- Timeout: 10 seconds
- Response: JSON with status, timestamp, details

### Question 2: Rate Limits
**Proposed**:
| Endpoint | Per Minute | Per Hour |
|----------|-----------|----------|
| Deploy | 10 | 100 |
| Restart | 5 | 50 |
| Logs | 20 | 200 |
| Status Check | 60 | 3600 |

### Question 3: Log Redaction
**Proposed**: 6 pattern categories + 20 env var blacklist
- Patterns: AWS creds, DB passwords, API tokens, PII, credit cards, SSH keys
- Implementation: Regex-based service with redaction middleware
- Applied to all log retrieval endpoints

### Question 4: Container→App Code Mapping
**Proposed**: 
- Canonical source: `app_code` (from Stacker project metadata)
- Storage: User Service `deployment_apps` table (new)
- 1:1 mapping per deployment

---

## Implementation Timeline

**Priority 1 (This Week)**:
- [ ] Team reviews and confirms all proposals
- [ ] Coordinate with User Service on `deployment_apps` schema
- [ ] Begin health check endpoint implementation

**Priority 2 (Next Week)**:
- [ ] Implement health check endpoint in Stacker
- [ ] Add log redaction service
- [ ] Create rate limiter middleware
- [ ] Update User Service deployment creation logic

**Priority 3**:
- [ ] Integration tests
- [ ] Status Panel updates to use new endpoints
- [ ] Documentation and monitoring

---

## Artifacts

- **Main Proposal Document**: [docs/OPEN_QUESTIONS_RESOLUTIONS.md](OPEN_QUESTIONS_RESOLUTIONS.md)
- **Updated TODO**: [TODO.md](../TODO.md) (lines 8-21)
- **Internal Tracking**: `/memories/open_questions.md`

---

## Coordination

To provide feedback or request changes:

1. **Review** [OPEN_QUESTIONS_RESOLUTIONS.md](OPEN_QUESTIONS_RESOLUTIONS.md) fully
2. **Comment** in TODO.md with specific concerns
3. **Notify** team via `/memories/open_questions.md` update
4. **Coordinate** with User Service and Status Panel teams for schema/contract alignment

---

## Key Decisions Made

✅ **Health Check Design**: REST endpoint (not webhook) for async polling by Status Panel  
✅ **Rate Limiting**: Redis-backed per-user limits (not IP-based) for flexibility  
✅ **Log Security**: Whitelist approach (redact known sensitive patterns) for safety  
✅ **App Mapping**: Database schema (deployment_apps) for fast lookups vs. parsing JSON  

---

## Questions Answered

| # | Question | Status | Details |
|---|----------|--------|---------|
| 1 | Health check contract | ✅ Proposed | REST endpoint with 10s timeout |
| 2 | Rate limits | ✅ Proposed | Deploy 10/min, Restart 5/min, Logs 20/min |
| 3 | Log redaction | ✅ Proposed | 6 patterns + 20 env var blacklist |
| 4 | Container mapping | ✅ Proposed | `app_code` canonical, new User Service table |

---

**Next Action**: Await team review and confirmation of proposals.
