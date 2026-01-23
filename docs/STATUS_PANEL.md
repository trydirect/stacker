# Status Panel / Stacker Endpoint Cheatsheet

This doc lists the Stacker endpoints used by the Status Panel flow, plus minimal curl examples. Replace placeholders like `<TOKEN>`, `<DEPLOYMENT_HASH>`, `<APP_CODE>` as needed.

## Auth Overview
- User/UI calls (`/api/v1/commands...`): OAuth Bearer token in `Authorization: Bearer <TOKEN>`; caller must be `group_user` or `group_admin` per Casbin rules.
- Agent calls (`/api/v1/agent/...`): Bearer token returned by agent registration; include `X-Agent-Id`. POSTs should also include HMAC headers (`X-Timestamp`, `X-Request-Id`, `X-Agent-Signature`) if enabled.

## User-Facing (UI) Endpoints
These are used by the dashboard/Blog UI to request logs/health/restart and to read results.

### Create command (health, logs, restart)
- `POST /api/v1/commands`
- Headers: `Authorization: Bearer <TOKEN>`, `Content-Type: application/json`
- Body examples:
  - Logs
    ```bash
    curl -X POST http://localhost:8000/api/v1/commands \
      -H "Authorization: Bearer <TOKEN>" \
      -H "Content-Type: application/json" \
      -d '{
        "deployment_hash": "<DEPLOYMENT_HASH>",
        "command_type": "logs",
        "parameters": {
          "app_code": "<APP_CODE>",
          "cursor": null,
          "limit": 400,
          "streams": ["stdout", "stderr"],
          "redact": true
        }
      }'
    ```
  - Health
    ```bash
    curl -X POST http://localhost:8000/api/v1/commands \
      -H "Authorization: Bearer <TOKEN>" \
      -H "Content-Type: application/json" \
      -d '{
        "deployment_hash": "<DEPLOYMENT_HASH>",
        "command_type": "health",
        "parameters": {
          "app_code": "<APP_CODE>",
          "include_metrics": true
        }
      }'
    ```
  - Restart
    ```bash
    curl -X POST http://localhost:8000/api/v1/commands \
      -H "Authorization: Bearer <TOKEN>" \
      -H "Content-Type: application/json" \
      -d '{
        "deployment_hash": "<DEPLOYMENT_HASH>",
        "command_type": "restart",
        "parameters": {
          "app_code": "<APP_CODE>",
          "force": false
        }
      }'
    ```

### List commands for a deployment (to read results)
- `GET /api/v1/commands/<DEPLOYMENT_HASH>`
- Headers: `Authorization: Bearer <TOKEN>`
- Example:
  ```bash
  curl -X GET http://localhost:8000/api/v1/commands/<DEPLOYMENT_HASH> \
    -H "Authorization: Bearer <TOKEN>"
  ```

### Get a specific command
- `GET /api/v1/commands/<DEPLOYMENT_HASH>/<COMMAND_ID>`
- Headers: `Authorization: Bearer <TOKEN>`
- Example:
  ```bash
  curl -X GET http://localhost:8000/api/v1/commands/<DEPLOYMENT_HASH>/<COMMAND_ID> \
    -H "Authorization: Bearer <TOKEN>"
  ```

### Fetch agent capabilities + availability (for UI gating)
- `GET /api/v1/deployments/<DEPLOYMENT_HASH>/capabilities`
- Headers: `Authorization: Bearer <TOKEN>`
- Response fields:
  - `status`: `online|offline`
  - `last_heartbeat`, `version`, `system_info`, `capabilities[]` (raw agent data)
  - `commands[]`: filtered command catalog entries `{type,label,icon,scope,requires}`
- Example:
  ```bash
  curl -X GET http://localhost:8000/api/v1/deployments/<DEPLOYMENT_HASH>/capabilities \
    -H "Authorization: Bearer <TOKEN>"
  ```

### Cancel a command
- `POST /api/v1/commands/<DEPLOYMENT_HASH>/<COMMAND_ID>/cancel`
- Headers: `Authorization: Bearer <TOKEN>`
- Example:
  ```bash
  curl -X POST http://localhost:8000/api/v1/commands/<DEPLOYMENT_HASH>/<COMMAND_ID>/cancel \
    -H "Authorization: Bearer <TOKEN>"
  ```

## Agent-Facing Endpoints
These are called by the Status Panel agent (runner) to receive work and report results.

### Register agent
- `POST /api/v1/agent/register`
- Headers: optional `X-Agent-Signature` if your flow signs registration
- Body (example): `{"deployment_hash":"<DEPLOYMENT_HASH>","system_info":{}}`
- Returns: `agent_id`, `agent_token`

### Wait for next command (long poll)
- `GET /api/v1/agent/commands/wait/<DEPLOYMENT_HASH>`
- Headers: `Authorization: Bearer <AGENT_TOKEN>`, `X-Agent-Id: <AGENT_ID>`
- Optional query: `timeout`, `priority`, `last_command_id`
- Example:
  ```bash
  curl -X GET "http://localhost:8000/api/v1/agent/commands/wait/<DEPLOYMENT_HASH>?timeout=30" \
    -H "Authorization: Bearer <AGENT_TOKEN>" \
    -H "X-Agent-Id: <AGENT_ID>" \
    -H "X-Agent-Version: <AGENT_VERSION>" \
    -H "Accept: application/json"
  ```

### Report command result
- `POST /api/v1/agent/commands/report`
- Headers: `Authorization: Bearer <AGENT_TOKEN>`, `X-Agent-Id: <AGENT_ID>`, `Content-Type: application/json` (+ HMAC headers if enabled)
- Body example for logs result:
  ```bash
  curl -X POST http://localhost:8000/api/v1/agent/commands/report \
    -H "Authorization: Bearer <AGENT_TOKEN>" \
    -H "X-Agent-Id: <AGENT_ID>" \
    -H "Content-Type: application/json" \
    -d '{
      "type": "logs",
      "deployment_hash": "<DEPLOYMENT_HASH>",
      "app_code": "<APP_CODE>",
      "cursor": "<NEXT_CURSOR>",
      "lines": [
        {"ts": "2024-01-01T00:00:00Z", "stream": "stdout", "message": "hello", "redacted": false}
      ],
      "truncated": false
    }'
  ```

## Notes
- Allowed command types are fixed: `health`, `logs`, `restart`.
- For log commands, `app_code` is required and `streams` must be a subset of `stdout|stderr`; `limit` must be 1-1000.
- UI should only talk to `/api/v1/commands...`; agent-only calls use `/api/v1/agent/...`.





To hand a command to the remote Status Panel agent:

User/UI side: enqueue the command in Stacker
POST /api/v1/commands with the command payload (e.g., logs/health/restart). This writes to commands + command_queue.
Auth: user OAuth Bearer.
Agent pickup (Status Panel agent)
The agent long-polls GET /api/v1/agent/commands/wait/{deployment_hash} with Authorization: Bearer <agent_token> and X-Agent-Id. It receives the queued command (type + parameters).
Optional query: timeout, priority, last_command_id.
Agent executes and reports back
Agent runs the command against the stack and POSTs /api/v1/agent/commands/report with the result body (logs/health/restart schema).
Headers: Authorization: Bearer <agent_token>, X-Agent-Id, and, if enabled, HMAC headers (X-Timestamp, X-Request-Id, X-Agent-Signature).
UI reads results
Poll GET /api/v1/commands/{deployment_hash} to retrieve the command result (lines/cursor for logs, status/metrics for health, etc.).
