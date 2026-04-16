# DAG Pipes — Developer Manual

Build, validate, and execute data pipelines across your deployed services using the **stacker CLI** or the **Visual DAG Editor** (web UI).

> **What is a DAG Pipe?**
> A Directed Acyclic Graph (DAG) where each node is a processing step (source, transform, condition, target) and edges define the data flow between them. Stacker executes steps level-by-level — steps at the same topological level run in parallel.

---

## Table of Contents

1. [Concepts](#concepts)
2. [Step Types Reference](#step-types-reference)
3. [Tutorial: OpenClaw + PostgreSQL CDC Pipeline](#tutorial-openclaw--postgresql-cdc-pipeline)
   - [Scenario](#scenario)
   - [Method 1: CLI Workflow](#method-1-cli-workflow)
   - [Method 2: Visual DAG Editor (Web)](#method-2-visual-dag-editor-web)
4. [CLI Command Reference](#cli-command-reference)
5. [REST API Reference](#rest-api-reference)
6. [gRPC Streaming](#grpc-streaming)
7. [Troubleshooting](#troubleshooting)

---

## Concepts

### Templates vs Instances

| | Template | Instance |
|---|---------|----------|
| **What** | Reusable pipeline definition (steps, edges, config) | Deployment-specific activation |
| **Scope** | Project-level, shareable | Tied to a `deployment_hash` |
| **Contains** | DAG steps, edges, field mappings | Status, trigger counts, execution history |

### Execution Model

```
Template (DAG definition)
   ↓  create instance
Instance (bound to deployment)
   ↓  trigger / activate
Execution (one run of the pipeline)
   ↓  topological sort → level-by-level
Step Executions (per-step input/output/status)
```

Steps are grouped by **topological level** (Kahn's algorithm). All steps in the same level execute in parallel. Each step receives the merged output of all its upstream dependencies.

### Validation Rules

- At least **one source step** required (`source`, `ws_source`, `http_stream_source`, `grpc_source`, `cdc_source`, `amqp_source`, `kafka_source`)
- At least **one target step** required (`target`, `ws_target`, `grpc_target`)
- **No cycles** allowed (it's a DAG)
- All step types must be from the [supported list](#step-types-reference)

---

## Step Types Reference

### Sources (data ingestion)

| Type | Icon | Description | Key Config Fields |
|------|------|-------------|-------------------|
| `source` | 📥 | Generic REST/JSON data source | `url`, `method`, `headers`, `output` |
| `ws_source` | 🔌 | WebSocket consumer | `url` |
| `http_stream_source` | 🌊 | Server-Sent Events (SSE) listener | `url`, `event_filter` |
| `grpc_source` | ⚡ | gRPC server-streaming subscriber | `endpoint`, `pipe_instance_id`, `step_id` |
| `cdc_source` | 🔄 | PostgreSQL Change Data Capture | `replication_slot`, `publication`, `tables` |
| `amqp_source` | 🐰 | RabbitMQ / AMQP consumer | `queue`, `exchange`, `routing_key` |
| `kafka_source` | 📨 | Apache Kafka consumer | `brokers`, `topic`, `group_id` |

### Processing (transform & route)

| Type | Icon | Description | Key Config Fields |
|------|------|-------------|-------------------|
| `transform` | 🔀 | Field mapping / data transformation | `field_mapping` (JSONPath expressions) |
| `condition` | ❓ | Conditional branching | `field`, `operator`, `value` |
| `parallel_split` | ⑃ | Fork into parallel branches | *(none — structural node)* |
| `parallel_join` | ⑂ | Merge parallel branches | *(none — structural node)* |

### Targets (data delivery)

| Type | Icon | Description | Key Config Fields |
|------|------|-------------|-------------------|
| `target` | 📤 | Generic REST/HTTP sink | `url`, `method`, `headers` |
| `ws_target` | 🔌 | WebSocket sender | `url` |
| `grpc_target` | ⚡ | gRPC unary call | `endpoint`, `pipe_instance_id`, `step_id` |

### Condition Operators

Used in `condition` steps:

| Operator | Meaning | Example |
|----------|---------|---------|
| `eq` | Equals | `{"field": "status", "operator": "eq", "value": "active"}` |
| `ne` | Not equals | `{"field": "type", "operator": "ne", "value": "draft"}` |
| `gt` | Greater than | `{"field": "score", "operator": "gt", "value": 80}` |
| `lt` | Less than | `{"field": "retries", "operator": "lt", "value": 3}` |
| `gte` | Greater or equal | `{"field": "priority", "operator": "gte", "value": 5}` |
| `lte` | Less or equal | `{"field": "age", "operator": "lte", "value": 30}` |

---

## Tutorial: OpenClaw + PostgreSQL CDC Pipeline

### Scenario

You have a deployed stack with:

- **OpenClaw** — AI workbench (REST API on port 8080)
- **PostgreSQL** — database backend
- **Redis** — caching layer
- **Telegraf** — metrics collection

**Goal**: Build a pipeline that:

1. Captures database changes (INSERT/UPDATE) from PostgreSQL via CDC
2. Filters only changes to the `ai_sessions` table
3. Transforms the data into a notification payload
4. Sends it to OpenClaw's webhook endpoint for real-time model re-training triggers
5. Also streams the event via WebSocket for a live monitoring dashboard

The resulting DAG:

```
[cdc_source: pg_changes]
        │
        ▼
[condition: is_ai_session]
        │
   ┌────┴────┐
   ▼         ▼
[transform: [parallel_split]
 enrich]     │
   │    ┌────┴────┐
   │    ▼         ▼
   │ [target:  [ws_target:
   │  openclaw]  dashboard]
   │    │         │
   │    └────┬────┘
   │         ▼
   │  [parallel_join]
   │         │
   └────►────┘
```

---

### Method 1: CLI Workflow

#### Step 1 — Create the pipe template

```bash
# Login to stacker (if not already authenticated)
stacker login

# Create a named pipe template for our project
curl -s -X POST http://localhost:8080/api/v1/pipes/templates \
  -H "Authorization: Bearer $(stacker token)" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "OpenClaw CDC Retraining Pipeline",
    "description": "Captures PostgreSQL changes and triggers OpenClaw model retraining",
    "source_app_type": "postgresql",
    "target_app_type": "openclaw"
  }' | jq .
```

Save the returned `template_id`:

```bash
TEMPLATE_ID="<returned-uuid>"
```

#### Step 2 — Add DAG steps

```bash
API="http://localhost:8080/api/v1/pipes/$TEMPLATE_ID/dag"
AUTH="Authorization: Bearer $(stacker token)"

# Step 1: CDC Source — listen to PostgreSQL WAL
curl -s -X POST "$API/steps" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d '{
    "name": "pg_changes",
    "step_type": "cdc_source",
    "step_order": 1,
    "config": {
      "replication_slot": "openclaw_pipe_slot",
      "publication": "openclaw_pub",
      "tables": ["public.ai_sessions", "public.training_jobs"]
    }
  }' | jq -r '.item.id'
# → Save as CDC_STEP_ID

# Step 2: Condition — filter for ai_sessions table only
curl -s -X POST "$API/steps" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d '{
    "name": "is_ai_session",
    "step_type": "condition",
    "step_order": 2,
    "config": {
      "field": "table_name",
      "operator": "eq",
      "value": "ai_sessions"
    }
  }' | jq -r '.item.id'
# → Save as CONDITION_STEP_ID

# Step 3: Transform — enrich CDC event for OpenClaw
curl -s -X POST "$API/steps" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d '{
    "name": "enrich_payload",
    "step_type": "transform",
    "step_order": 3,
    "config": {
      "field_mapping": {
        "event_type": "$.operation",
        "session_id": "$.after.id",
        "model_name": "$.after.model_name",
        "parameters": "$.after.parameters",
        "timestamp": "$.captured_at",
        "action": "retrain_trigger"
      }
    }
  }' | jq -r '.item.id'
# → Save as TRANSFORM_STEP_ID

# Step 4: Parallel split — fan out to two targets
curl -s -X POST "$API/steps" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d '{
    "name": "fan_out",
    "step_type": "parallel_split",
    "step_order": 4,
    "config": {}
  }' | jq -r '.item.id'
# → Save as SPLIT_STEP_ID

# Step 5: Target — POST to OpenClaw retraining webhook
curl -s -X POST "$API/steps" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d '{
    "name": "openclaw_webhook",
    "step_type": "target",
    "step_order": 5,
    "config": {
      "url": "http://openclaw:8080/api/v1/hooks/retrain",
      "method": "POST",
      "headers": {
        "Content-Type": "application/json",
        "X-Pipe-Source": "cdc"
      }
    }
  }' | jq -r '.item.id'
# → Save as TARGET_STEP_ID

# Step 6: WebSocket target — stream to monitoring dashboard
curl -s -X POST "$API/steps" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d '{
    "name": "dashboard_stream",
    "step_type": "ws_target",
    "step_order": 6,
    "config": {
      "url": "ws://dashboard:3000/ws/events"
    }
  }' | jq -r '.item.id'
# → Save as WS_TARGET_STEP_ID

# Step 7: Parallel join — merge branches
curl -s -X POST "$API/steps" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d '{
    "name": "merge",
    "step_type": "parallel_join",
    "step_order": 7,
    "config": {}
  }' | jq -r '.item.id'
# → Save as JOIN_STEP_ID

# Step 8: Final target — log completion
curl -s -X POST "$API/steps" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d '{
    "name": "completion_log",
    "step_type": "target",
    "step_order": 8,
    "config": {
      "url": "http://telegraf:8186/write",
      "method": "POST",
      "headers": {"Content-Type": "text/plain"},
      "body_template": "pipe_execution,pipeline=openclaw_cdc status=\"completed\""
    }
  }' | jq -r '.item.id'
# → Save as LOG_TARGET_ID
```

#### Step 3 — Connect steps with edges

```bash
# CDC → Condition
curl -s -X POST "$API/edges" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"from_step_id\": \"$CDC_STEP_ID\", \"to_step_id\": \"$CONDITION_STEP_ID\"}"

# Condition → Transform
curl -s -X POST "$API/edges" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"from_step_id\": \"$CONDITION_STEP_ID\", \"to_step_id\": \"$TRANSFORM_STEP_ID\"}"

# Transform → Parallel Split
curl -s -X POST "$API/edges" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"from_step_id\": \"$TRANSFORM_STEP_ID\", \"to_step_id\": \"$SPLIT_STEP_ID\"}"

# Split → OpenClaw target
curl -s -X POST "$API/edges" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"from_step_id\": \"$SPLIT_STEP_ID\", \"to_step_id\": \"$TARGET_STEP_ID\"}"

# Split → WebSocket dashboard
curl -s -X POST "$API/edges" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"from_step_id\": \"$SPLIT_STEP_ID\", \"to_step_id\": \"$WS_TARGET_STEP_ID\"}"

# OpenClaw target → Join
curl -s -X POST "$API/edges" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"from_step_id\": \"$TARGET_STEP_ID\", \"to_step_id\": \"$JOIN_STEP_ID\"}"

# WebSocket target → Join
curl -s -X POST "$API/edges" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"from_step_id\": \"$WS_TARGET_STEP_ID\", \"to_step_id\": \"$JOIN_STEP_ID\"}"

# Join → Completion log
curl -s -X POST "$API/edges" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"from_step_id\": \"$JOIN_STEP_ID\", \"to_step_id\": \"$LOG_TARGET_ID\"}"
```

#### Step 4 — Validate the DAG

```bash
curl -s -X POST "$API/validate" \
  -H "$AUTH" -H "Content-Type: application/json" | jq .
```

Expected response:
```json
{
  "valid": true,
  "total_steps": 8,
  "execution_levels": 5,
  "sources": ["cdc_source"],
  "targets": ["target", "ws_target"]
}
```

#### Step 5 — Create an instance and execute

```bash
# Create a pipe instance bound to your deployment
INSTANCE=$(curl -s -X POST http://localhost:8080/api/v1/pipes/instances \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d "{
    \"pipe_template_id\": \"$TEMPLATE_ID\",
    \"deployment_hash\": \"$(stacker config get deployment_hash)\",
    \"name\": \"OpenClaw CDC — Production\"
  }" | jq -r '.item.id')

# Execute the DAG (manual trigger with test data)
curl -s -X POST "http://localhost:8080/api/v1/pipes/instances/$INSTANCE/dag/execute" \
  -H "$AUTH" -H "Content-Type: application/json" \
  -d '{
    "input_data": {
      "table_name": "ai_sessions",
      "operation": "INSERT",
      "after": {
        "id": "sess-001",
        "model_name": "gpt-4-finetune",
        "parameters": {"epochs": 3, "learning_rate": 0.001}
      },
      "captured_at": "2026-04-16T12:00:00Z"
    }
  }' | jq .
```

Expected execution result:
```json
{
  "execution_id": "...",
  "status": "completed",
  "total_steps": 8,
  "completed_steps": 8,
  "failed_steps": 0,
  "skipped_steps": 0,
  "execution_order": ["pg_changes", "is_ai_session", "enrich_payload", "fan_out", "openclaw_webhook", "dashboard_stream", "merge", "completion_log"]
}
```

#### Step 6 — Activate for continuous CDC listening

```bash
# Activate with webhook trigger — agent will listen for CDC events
stacker pipe activate $INSTANCE --trigger webhook
```

#### All-in-one script

For convenience, here's the complete pipeline as a shell script:

```bash
#!/bin/bash
# create-openclaw-cdc-pipeline.sh
set -euo pipefail

BASE="http://localhost:8080/api/v1"
AUTH="Authorization: Bearer $(stacker token)"
CT="Content-Type: application/json"

echo "Creating pipe template..."
TEMPLATE_ID=$(curl -sf -X POST "$BASE/pipes/templates" \
  -H "$AUTH" -H "$CT" \
  -d '{"name":"OpenClaw CDC Pipeline","description":"CDC → filter → transform → OpenClaw + dashboard"}' \
  | jq -r '.item.id')
echo "Template: $TEMPLATE_ID"

DAG="$BASE/pipes/$TEMPLATE_ID/dag"

add_step() { curl -sf -X POST "$DAG/steps" -H "$AUTH" -H "$CT" -d "$1" | jq -r '.item.id'; }
add_edge() { curl -sf -X POST "$DAG/edges" -H "$AUTH" -H "$CT" -d "$1"; }

echo "Adding steps..."
S1=$(add_step '{"name":"pg_changes","step_type":"cdc_source","step_order":1,"config":{"replication_slot":"openclaw_pipe_slot","publication":"openclaw_pub","tables":["public.ai_sessions"]}}')
S2=$(add_step '{"name":"is_ai_session","step_type":"condition","step_order":2,"config":{"field":"table_name","operator":"eq","value":"ai_sessions"}}')
S3=$(add_step '{"name":"enrich","step_type":"transform","step_order":3,"config":{"field_mapping":{"event_type":"$.operation","session_id":"$.after.id","model_name":"$.after.model_name","action":"retrain_trigger"}}}')
S4=$(add_step '{"name":"fan_out","step_type":"parallel_split","step_order":4,"config":{}}')
S5=$(add_step '{"name":"openclaw_webhook","step_type":"target","step_order":5,"config":{"url":"http://openclaw:8080/api/v1/hooks/retrain","method":"POST"}}')
S6=$(add_step '{"name":"dashboard_ws","step_type":"ws_target","step_order":6,"config":{"url":"ws://dashboard:3000/ws/events"}}')
S7=$(add_step '{"name":"merge","step_type":"parallel_join","step_order":7,"config":{}}')
S8=$(add_step '{"name":"log_to_telegraf","step_type":"target","step_order":8,"config":{"url":"http://telegraf:8186/write","method":"POST"}}')

echo "Connecting edges..."
add_edge "{\"from_step_id\":\"$S1\",\"to_step_id\":\"$S2\"}" > /dev/null
add_edge "{\"from_step_id\":\"$S2\",\"to_step_id\":\"$S3\"}" > /dev/null
add_edge "{\"from_step_id\":\"$S3\",\"to_step_id\":\"$S4\"}" > /dev/null
add_edge "{\"from_step_id\":\"$S4\",\"to_step_id\":\"$S5\"}" > /dev/null
add_edge "{\"from_step_id\":\"$S4\",\"to_step_id\":\"$S6\"}" > /dev/null
add_edge "{\"from_step_id\":\"$S5\",\"to_step_id\":\"$S7\"}" > /dev/null
add_edge "{\"from_step_id\":\"$S6\",\"to_step_id\":\"$S7\"}" > /dev/null
add_edge "{\"from_step_id\":\"$S7\",\"to_step_id\":\"$S8\"}" > /dev/null

echo "Validating..."
curl -sf -X POST "$DAG/validate" -H "$AUTH" -H "$CT" | jq .

echo ""
echo "✅ Pipeline ready! Template ID: $TEMPLATE_ID"
echo ""
echo "Next steps:"
echo "  1. Create instance:  curl -X POST $BASE/pipes/instances -d '{\"pipe_template_id\":\"$TEMPLATE_ID\",\"deployment_hash\":\"YOUR_HASH\",\"name\":\"Production CDC\"}'"
echo "  2. Execute:           curl -X POST $BASE/pipes/instances/INSTANCE_ID/dag/execute -d '{\"input_data\":{...}}'"
echo "  3. Activate:          stacker pipe activate INSTANCE_ID --trigger webhook"
```

---

### Method 2: Visual DAG Editor (Web)

The **Visual DAG Editor** provides a drag-and-drop interface for building the same pipeline — no curl commands needed.

#### Access

```
http://localhost:8080/editor
```

> **Demo Mode**: The editor works without authentication for local experimentation. A "Demo Mode" banner is shown — API calls are skipped, and changes exist only in the browser. Sign up or log in to persist pipelines.

#### Building the OpenClaw CDC Pipeline

1. **Start from a template** (optional):
   Click **"Use Template"** and select **"CDC Replicator"** as a starting point, then customize it.

2. **Or build from scratch**:

   a. **Drag sources from the palette** (left sidebar):
      - Drag **"CDC Source"** 🔄 onto the canvas

   b. **Configure the step** (click the node):
      - Replication Slot: `openclaw_pipe_slot`
      - Publication: `openclaw_pub`
      - Tables: `public.ai_sessions`

   c. **Add processing steps**:
      - Drag **"Condition"** ❓ → set field=`table_name`, operator=`eq`, value=`ai_sessions`
      - Drag **"Transform"** 🔀 → define field mappings
      - Drag **"Parallel Split"** ⑃ for fan-out

   d. **Add targets**:
      - Drag **"Target"** 📤 → set URL to `http://openclaw:8080/api/v1/hooks/retrain`
      - Drag **"WS Target"** 🔌 → set URL to `ws://dashboard:3000/ws/events`
      - Drag **"Parallel Join"** ⑂ to merge

   e. **Connect steps**: Click a node's output handle and drag to the next node's input handle.

   f. **Delete connections**: Select an edge and press **Delete** or **Backspace** key.

3. **Validate**: Click the **"Validate"** button — green toast = valid, red toast = errors.

4. **Execute**: Click **"Execute"** to trigger a test run.

#### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Delete` / `Backspace` | Delete selected edge or node |
| Drag from handle | Create connection |
| Click node | Open config panel |
| Scroll | Zoom in/out |
| Click + drag canvas | Pan |

---

## CLI Command Reference

| Command | Description |
|---------|-------------|
| `stacker pipe scan <app>` | Discover connectable API endpoints |
| `stacker pipe create <source> <target>` | Interactive pipe creation wizard |
| `stacker pipe list` | List all pipes for current deployment |
| `stacker pipe activate <pipe-id>` | Start listening (webhook/poll/manual) |
| `stacker pipe deactivate <pipe-id>` | Stop listener/scheduler |
| `stacker pipe trigger <pipe-id>` | One-shot manual execution |
| `stacker pipe history <instance-id>` | View execution history |
| `stacker pipe replay <execution-id>` | Re-run a previous execution |

### Common flags

| Flag | Available on | Description |
|------|-------------|-------------|
| `--json` | All commands | Machine-readable JSON output |
| `--deployment <hash>` | All commands | Override auto-detected deployment |
| `--trigger <type>` | `activate` | `webhook` (default), `poll`, `manual` |
| `--poll-interval <secs>` | `activate` | Poll frequency (default: 300s) |
| `--data <json>` | `trigger` | Override source data with custom JSON |
| `--capture-samples` | `scan` | Capture real API response samples |
| `--ai` / `--no-ai` / `--ml` | `create` | Field matching strategy |
| `--manual` | `create` | Skip auto-matching |
| `--limit <n>` | `history` | Max results (default: 20) |

---

## REST API Reference

### DAG Editing Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/pipes/templates` | Create pipe template |
| `GET` | `/api/v1/pipes/templates` | List templates |
| `GET` | `/api/v1/pipes/templates/{id}` | Get template |
| `DELETE` | `/api/v1/pipes/templates/{id}` | Delete template |
| `POST` | `/api/v1/pipes/{template_id}/dag/steps` | Add step to DAG |
| `GET` | `/api/v1/pipes/{template_id}/dag/steps` | List steps |
| `GET` | `/api/v1/pipes/{template_id}/dag/steps/{step_id}` | Get step |
| `PUT` | `/api/v1/pipes/{template_id}/dag/steps/{step_id}` | Update step |
| `DELETE` | `/api/v1/pipes/{template_id}/dag/steps/{step_id}` | Delete step |
| `POST` | `/api/v1/pipes/{template_id}/dag/edges` | Add edge |
| `GET` | `/api/v1/pipes/{template_id}/dag/edges` | List edges |
| `DELETE` | `/api/v1/pipes/{template_id}/dag/edges/{edge_id}` | Delete edge |
| `POST` | `/api/v1/pipes/{template_id}/dag/validate` | Validate DAG |

### Instance & Execution Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/pipes/instances` | Create instance |
| `GET` | `/api/v1/pipes/instances/{deployment_hash}` | List instances |
| `GET` | `/api/v1/pipes/instances/detail/{id}` | Get instance |
| `PUT` | `/api/v1/pipes/instances/{id}/status` | Update status |
| `DELETE` | `/api/v1/pipes/instances/{id}` | Delete instance |
| `POST` | `/api/v1/pipes/instances/{id}/dag/execute` | Execute DAG |
| `GET` | `/api/v1/pipes/instances/{id}/executions` | List executions |
| `GET` | `/api/v1/pipes/executions/{id}` | Get execution |
| `POST` | `/api/v1/pipes/executions/{id}/replay` | Replay execution |

### Streaming

| Protocol | Path | Description |
|----------|------|-------------|
| WebSocket | `/api/v1/pipes/instances/{id}/stream` | Live execution events |

### Resilience (Circuit Breaker + DLQ)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/pipes/*/dlq` | List dead-letter queue items |
| `POST` | `/api/v1/pipes/*/dlq/{id}/retry` | Retry failed item |
| `POST` | `/api/v1/pipes/*/dlq/{id}/discard` | Discard failed item |
| `GET` | `/api/v1/pipes/*/circuit-breaker` | Get circuit breaker state |
| `PUT` | `/api/v1/pipes/*/circuit-breaker` | Configure circuit breaker |
| `POST` | `/api/v1/pipes/*/circuit-breaker/reset` | Reset circuit breaker |

---

## gRPC Streaming

For high-throughput or real-time pipelines, use gRPC source/target steps instead of REST.

### Protocol (proto/pipe.proto)

```protobuf
service PipeService {
  rpc Send(PipeMessage) returns (PipeResponse);          // Unary — send to target
  rpc Subscribe(SubscribeRequest) returns (stream PipeMessage);  // Server-stream — source
}

message PipeMessage {
  string pipe_instance_id = 1;
  string step_id = 2;
  google.protobuf.Struct payload = 3;   // Arbitrary JSON as protobuf Struct
  int64 timestamp_ms = 4;
}
```

### Using gRPC steps in a DAG

```json
{
  "name": "live_feed",
  "step_type": "grpc_source",
  "config": {
    "endpoint": "http://grpc-service:50051",
    "pipe_instance_id": "...",
    "step_id": "..."
  }
}
```

The `grpc_source` subscribes to a server-streaming RPC and receives `PipeMessage` items. The `grpc_target` sends data via unary `Send` RPC.

---

## Troubleshooting

### "No source step found"
Your DAG needs at least one source type. Add a `source`, `cdc_source`, `amqp_source`, `kafka_source`, `ws_source`, `http_stream_source`, or `grpc_source` step.

### "No target step found"
Add a `target`, `ws_target`, or `grpc_target` step.

### "Cycle detected"
Edges form a loop. Remove the edge that creates the cycle. In the Visual Editor, select the edge and press Delete.

### Validate returns 401
You are not authenticated. Run `stacker login` or add a valid `Authorization: Bearer <token>` header. The Visual Editor's demo mode skips API calls — sign in to persist and validate.

### Step execution shows "failed"
Check the `error` field in the step execution response:
```bash
curl -s "$BASE/pipes/$TEMPLATE_ID/dag/executions/$EXEC_ID/steps" \
  -H "$AUTH" | jq '.[] | select(.status == "failed")'
```

### CDC source not receiving events
1. Verify PostgreSQL has logical replication enabled (`wal_level = logical` in postgresql.conf)
2. Check the replication slot exists: `SELECT * FROM pg_replication_slots;`
3. Check the publication exists: `SELECT * FROM pg_publication_tables;`

### AMQP source not consuming
1. Verify RabbitMQ is accessible at the configured host
2. Check the queue exists in RabbitMQ Management UI (port 15672)
3. Verify exchange and routing key match the publisher's configuration

### Kafka source not subscribing
1. Verify brokers are reachable: `kafkacat -b localhost:9092 -L`
2. Check the topic exists: `kafka-topics.sh --list --bootstrap-server localhost:9092`
3. Verify `group_id` doesn't conflict with other consumers
