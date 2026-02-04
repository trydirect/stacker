<a href="https://discord.gg/mNhsa8VdYX"><img alt="Discord" src="https://img.shields.io/discord/578119430391988232?label=discord"></a>
<br>
<div align="center">
<img width="300" src="https://repository-images.githubusercontent.com/448846514/3468f301-0ba6-4b61-9bf1-164c06c06b08"> 
 </div>

# Stacker Project Overview
Stacker - is an application that helps users to create custom IT solutions based on dockerized open 
source apps and user's custom applications docker containers. Users can build their own project of applications, and 
deploy the final result to their favorite clouds using TryDirect API. See [CHANGELOG.md](CHANGELOG.md) for the latest platform updates.

## Startup Banner
When you start the Stacker server, you'll see a welcome banner displaying version and configuration info:

```
 ██████ ████████ █████   ██████ ██   ██ ███████ ██████  
██         ██    ██   ██ ██     ██  ██  ██      ██   ██ 
███████    ██    ███████ ██     █████   █████   ██████  
     ██    ██    ██   ██ ██     ██  ██  ██      ██   ██ 
██████     ██    ██   ██  █████ ██   ██ ███████ ██   ██ 

╭────────────────────────────────────────────────────────╮
│  Stacker                                          │
│  Version: 0.2.1t                                      │
│  Build: 0.2.0                                 │
│  Edition: 2021                                       │
╰────────────────────────────────────────────────────────╯

📋 Configuration Loaded
  🌐 Server Address: http://127.0.0.1:8000
  📦 Ready to accept connections
```

This banner provides quick visibility into:
- **Version**: Current Stacker version
- **Build**: Build version information
- **Edition**: Rust edition used
- **Server Address**: Where the API server is listening
- **Status**: Server readiness

## Core Purpose
- Allows users to build projects using both open source and custom Docker containers
- Provides deployment capabilities to various cloud platforms through TryDirect API
- Helps manage and orchestrate Docker-based application stacks

## Main Components

1. **Project Structure**
- Web UI (Stack Builder)
- Command Line Interface
- RESTful API Backend

2. **Key Features**
- User Authentication (via TryDirect OAuth)
- API Client Management
- Cloud Provider Key Management
- Docker Compose Generation
- Project Rating System
- Project Deployment Management

3. **Technical Architecture**
- Written in Rust
- Uses PostgreSQL database
- Implements REST API endpoints
- Includes Docker image validation
- Supports project deployment workflows
- Has RabbitMQ integration for deployment status updates

4. **Data Models**
The core Project model includes:
- Unique identifiers (id, stack_id)
- User identification
- Project metadata (name, metadata, request_json)
- Timestamps (created_at, updated_at)

5. **API Endpoints (user-facing)**
- `/project` - Project management
- `/project/deploy` - Deployment handling
- `/project/deploy/status` - Deployment status tracking
- `/rating` - Rating system
- `/client` - API client management

6. **Agent + Command Flow (self-hosted runner)**
- Register agent (no auth required): `POST /api/v1/agent/register`
  - Body: `deployment_hash`, optional `capabilities`, `system_info`
  - Response: `agent_id`, `agent_token`
- Agent long-poll for commands: `GET /api/v1/agent/commands/wait/:deployment_hash`
  - Headers: `X-Agent-Id: <agent_id>`, `Authorization: Bearer <agent_token>`
  - Optional query params: `timeout` (seconds), `interval` (seconds)
- Agent report command result: `POST /api/v1/agent/commands/report`
  - Headers: `X-Agent-Id`, `Authorization: Bearer <agent_token>`
  - Body: `command_id`, `deployment_hash`, `status` (`completed|failed`), `result`/`error`, optional `started_at`, required `completed_at`
- **Get deployment snapshot**: `GET /api/v1/agent/deployments/:deployment_hash`
  - Query params (optional):
    - `command_limit` (default: 50) - Number of recent commands to return
    - `include_command_results` (default: false) - Whether to include command result/error fields
  - Response: `agent`, `commands`, `containers`, `apps`
  - **Note**: Use `include_command_results=false` (default) for lightweight snapshots to avoid large payloads when commands contain log data
- Create command (user auth via OAuth Bearer): `POST /api/v1/commands`
  - Body: `deployment_hash`, `command_type`, `priority` (`low|normal|high|critical`), `parameters`, optional `timeout_seconds`
- **List commands for a deployment**: `GET /api/v1/commands/:deployment_hash`
  - Query params (optional):
    - `limit` (default: 50, max: 500) - Number of commands to return
    - `include_results` (default: false) - Whether to include command result/error fields
    - `since` (ISO 8601 timestamp) - Only return commands updated after this time
    - `wait_ms` (max: 30000) - Long-poll timeout when using `since`
  - Response: `list` of commands
  - **Note**: Use `include_results=true` when you need log data or command execution results

7. **Stacker → Agent HMAC-signed POSTs (v2)**
- All POST calls from Stacker to the agent must be signed per [STACKER_INTEGRATION_REQUIREMENTS.md](STACKER_INTEGRATION_REQUIREMENTS.md)
- Required headers: `X-Agent-Id`, `X-Timestamp`, `X-Request-Id`, `X-Agent-Signature`
- Signature: base64(HMAC_SHA256(AGENT_TOKEN, raw_body_bytes))
- Helper available: `helpers::AgentClient`
 - Base URL: set `AGENT_BASE_URL` to point Stacker at the target agent (e.g., `http://agent:5000`).

Example:
```rust
use stacker::helpers::AgentClient;
use serde_json::json;

let client = AgentClient::new("http://agent:5000", agent_id, agent_token);
let payload = json!({"deployment_hash": dh, "type": "restart_service", "parameters": {"service": "web"}});
let resp = client.get("/api/v1/status").await?;
``` 

### Pull-Only Command Architecture

Stacker uses a pull-only architecture for agent communication. **Stacker never dials out to agents.** Commands are enqueued in the database; agents poll and sign their own requests.

**Flow:**
1. UI/API calls `POST /api/v1/commands` or `POST /api/v1/agent/commands/enqueue`
2. Command is inserted into `commands` + `command_queue` tables
3. Agent polls `GET /api/v1/agent/commands/wait/{deployment_hash}` with HMAC headers
4. Stacker verifies agent's HMAC, returns queued commands
5. Agent executes locally and calls `POST /api/v1/agent/commands/report`

**Note:** `AGENT_BASE_URL` environment variable is NOT required for Status Panel commands.

Token rotation (writes to Vault; agent pulls latest):
```rust
use stacker::services::agent_dispatcher;

// Rotate token - stored in Vault, agent fetches on next poll
agent_dispatcher::rotate_token(&pg, &vault, &deployment_hash, "NEW_TOKEN").await?;
```

Console token rotation:
```bash
cargo run --bin console -- Agent rotate-token \
  --deployment-hash <hash> \
  --new-token <NEW_TOKEN>
```

### Configuration: Vault
- In configuration.yaml.dist, set:
  - vault.address: Vault URL (e.g., http://127.0.0.1:8200)
  - vault.token: Vault access token (dev/test only)
  - vault.agent_path_prefix: KV mount/prefix for agent tokens (e.g., agent or kv/agent)
- Environment variable overrides (optional): VAULT_ADDRESS, VAULT_TOKEN, VAULT_AGENT_PATH_PREFIX
- Agent tokens are stored at: {vault.agent_path_prefix}/{deployment_hash}/token

### Configuration: Agent Polling & Casbin Reload
- `agent_command_poll_timeout_secs` (default 30)
- `agent_command_poll_interval_secs` (default 3)
- `casbin_reload_enabled` (default true)
- `casbin_reload_interval_secs` (default 10)

Environment overrides:
- `STACKER_AGENT_POLL_TIMEOUT_SECS`
- `STACKER_AGENT_POLL_INTERVAL_SECS`
- `STACKER_CASBIN_RELOAD_ENABLED`
- `STACKER_CASBIN_RELOAD_INTERVAL_SECS`

The project appears to be a sophisticated orchestration platform that bridges the gap between Docker container management and cloud deployment, with a focus on user-friendly application stack building and management.

This is a high-level overview based on the code snippets provided. The project seems to be actively developed with features being added progressively, as indicated by the TODO sections in the documentation.


## How to start 


## Structure

Stacker (User's dashboard) - Management 
----------
Authentication through TryDirect OAuth
/api/auth checks client's creds, api token, api secret
/apiclient (Create/Manage user's api clients) example: BeerMaster (email, callback)
/rating   


Stacker (API) - Serves API clients 
----------
Authentication made through TryDirect OAuth, here we have only client 
Database (Read only)
Logging/Tracing (Files) / Quickwit for future 
/project (WebUI, as a result we have a JSON)
/project/deploy -> sends deploy command to TryDirect Install service 
/project/deploy/status - get installation progress (rabbitmq client),

#### TODO 
Find out how to get user's token for queue
Limit Requests Frequency (Related to user's settings/role/group etc)
Callback event, example: subscribe on get_report (internally from rabbitmq client),


main client (AllMasters) ->  client (Beermaster) 


#### Run db migration

```
sqlx migrate run

```


#### Down migration

```
sqlx migrate revert 
```


## Testing

Stacker ships targeted tests for the new User Service marketplace integrations. Run them with:

```
cargo test user_service_client
cargo test marketplace_webhook
cargo test deployment_validator
```

Each suite uses WireMock-backed HTTP servers, so they run offline and cover the actual request/response flows for the connector, webhook sender, and deployment validator.


## CURL examples


#### Authentication 


curl -X POST 


#### Rate Product 

```

 curl -vX POST 'http://localhost:8000/rating' -d '{"obj_id": 1, "category": "application", "comment":"some comment", "rate": 10}' --header 'Content-Type: application/json'

```


#### Deploy 
```
curl -X POST -H "Content-Type: application/json" -d @tests/mock_data/custom-stack-payload.json http://127.0.0.1:8000/project -H "Authorization: Bearer $TD_BEARER"
```


#### Create API Client
```
curl -X POST http://localhost:8000/client  --header 'Content-Type: application/json' -H "Authorization: Bearer $TD_BEARER"
```


test client deploy
http://localhost:8000/test/deploy


Test casbin rule
```
cargo r --bin console --features=explain debug casbin --path /client --action POST --subject admin_petru
```



"cargo sqlx prepare" requires setting the DATABASE_URL environment variable to a valid database URL. 

## TODOs
```
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/stacker
```
