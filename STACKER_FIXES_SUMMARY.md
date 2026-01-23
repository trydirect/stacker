# Stacker Backend Fixes - Status Panel Integration

**Date**: January 13, 2026  
**Target Team**: Status Panel / Frontend Teams  
**Status**: ✅ Ready for deployment

---

## Problem Identified

Status Panel was showing "Awaiting health data" indefinitely. Health commands were being created (201 responses) but never reaching the deployment agent for execution.

**Root Cause**: Database schema design flaw in command queueing system.
- `command_queue.command_id` column was UUID type
- Referenced `commands(id)` instead of `commands(command_id)`
- Type mismatch (UUID vs VARCHAR) prevented successful INSERT operations
- Commands appeared created in database but never reached the queue

---

## Fixes Applied

### 1. Database Schema Correction
**Migration**: `20260113000001_fix_command_queue_fk.up.sql`

```sql
-- Changed foreign key reference
ALTER TABLE command_queue DROP CONSTRAINT command_queue_command_id_fkey;
ALTER TABLE command_queue ALTER COLUMN command_id TYPE VARCHAR(64);
ALTER TABLE command_queue ADD CONSTRAINT command_queue_command_id_fkey 
  FOREIGN KEY (command_id) REFERENCES commands(command_id) ON DELETE CASCADE;
```

**Impact**: Commands now successfully insert into queue with correct type matching.

### 2. Timestamp Type Fix
**Migration**: `20260113000002_fix_audit_log_timestamp.up.sql`

```sql
-- Fixed type mismatch preventing audit log inserts
ALTER TABLE audit_log ALTER COLUMN created_at TYPE TIMESTAMPTZ;
```

**Impact**: Audit logging works correctly without type conversion errors.

### 3. Logging Improvements
**File**: `src/routes/command/create.rs`

Enhanced logging around `add_to_queue()` operation changed from debug to info level for production visibility:
- `"Attempting to add command {id} to queue"` 
- `"Successfully added command {id} to queue"` (on success)
- `"Failed to add command {id} to queue: {error}"` (on failure)

---

## What's Now Working ✅

### Command Creation Flow
```
UI Request (POST /api/v1/commands)
  ↓
Save command to database ✅
  ↓
Add to command_queue ✅
  ↓
Return 201 response with command_id ✅
```

### Agent Polling
```
Agent (GET /api/v1/agent/commands/wait/{deployment_hash})
  ↓
Query command_queue ✅
  ↓
Find queued commands ✅
  ↓
Fetch full command details ✅
  ↓
Return command to agent ✅
```

### Status Flow
```
Status Panel (GET /apps/status)
  ↓
Command exists with status: "queued" ✅
  ↓
Agent polls and retrieves command
  ↓
Agent executes health check
  ↓
Status updates to "running"/"stopped"
  ↓
Logs populated with results
```

---

## What Still Needs Implementation

### Stacker Agent Team Must:

1. **Execute Queued Commands**
   - When agent retrieves command from queue, execute health check
   - Capture stdout/stderr from execution
   - Collect container status from deployment

2. **Update Command Results**
   - POST command results back to Stacker API endpoint
   - Include status (running/stopped/error)
   - Include logs from execution output

3. **Update App Status**
   - Call `/apps/status` update endpoint with:
     - `status: "running" | "stopped" | "error"`
     - `logs: []` with execution output
     - `timestamp` of last check

**Verification**: Check Stacker logs for execution of commands from queue after agent polling.

---

## Testing

### To Verify Fixes:
```bash
# 1. Create health command
curl -X POST http://localhost:8000/api/v1/commands \
  -H "Content-Type: application/json" \
  -d '{
    "deployment_hash": "...",
    "command_type": "health",
    "parameters": {"app_code": "fastapi"}
  }'

# Response: 201 with command_id and status: "queued"

# 2. Check Stacker logs for:
# "[ADD COMMAND TO QUEUE - START]"
# "[ADDING COMMAND TO QUEUE - EVENT] sqlx::query"
# "rows_affected: 1"
# "[Successfully added command ... to queue]"

# 3. Agent should poll and retrieve within ~2 seconds
```

---

## Database Migrations Applied

Run these on production:
```bash
sqlx migrate run
```

Includes:
- `20260113000001_fix_command_queue_fk.up.sql`
- `20260113000002_fix_audit_log_timestamp.up.sql`

---

## Impact Summary

| Component | Before | After |
|-----------|--------|-------|
| Command Creation | ✅ Works | ✅ Works |
| Queue Insert | ❌ Silent failure | ✅ Works |
| Agent Poll | ❌ Returns 0 rows | ✅ Returns queued commands |
| Status Updates | ❌ Stuck "unknown" | 🔄 Awaiting agent execution |
| Logs | ❌ Empty | 🔄 Awaiting agent data |

---

## Deployment Checklist

- [ ] Apply migrations: `sqlx migrate run`
- [ ] Rebuild Stacker: `cargo build --release`
- [ ] Push new image: `docker build && docker push`
- [ ] Restart Stacker container
- [ ] Verify command creation returns 201
- [ ] Monitor logs for queue insertion success
- [ ] Coordinate with Stacker agent team on execution implementation

---

## Questions / Contact

For database/API issues: Backend team  
For agent execution: Stacker agent team  
For Status Panel integration: This documentation

