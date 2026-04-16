# DAG Pipes — Developer Manual

Build, validate, and execute data pipelines across your deployed services using the **stacker CLI** or the **Visual DAG Editor** (web UI).

> **What is a DAG Pipe?**
> A Directed Acyclic Graph (DAG) where each node is a processing step (source, transform, condition, target) and edges define the data flow between them. Stacker executes steps level-by-level — steps at the same topological level run in parallel.

---

## Table of Contents

1. [Concepts](#concepts)
2. [Step Types Reference](#step-types-reference)
3. [Examples](#examples)
   - [Example 1: Contact Form → Telegram + Slack](#example-1-contact-form--telegram--slack)
   - [Example 2: Contact Form → PostgreSQL → CDC → Telegram](#example-2-contact-form--postgresql--cdc--telegram)
   - [Example 3: Contact Form → Email + Slack](#example-3-contact-form--email--slack)
4. [Visual DAG Editor (Web)](#visual-dag-editor-web)
5. [CLI Command Reference](#cli-command-reference)
6. [REST API Reference](#rest-api-reference)
7. [gRPC Streaming](#grpc-streaming)
8. [Troubleshooting](#troubleshooting)

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

## Examples

All examples below use the same pattern:

```bash
# Setup variables (used in all examples)
BASE="http://localhost:8080/api/v1"
AUTH="Authorization: Bearer $(stacker token)"
CT="Content-Type: application/json"
```

> **Prefer a visual approach?** All examples below can also be built in the [Visual DAG Editor](#visual-dag-editor-web) at `http://localhost:8080/editor` using drag-and-drop — no curl needed.

---

### Example 1: Contact Form → Telegram + Slack

The simplest multi-target pipeline. A website form submission is sent to both Telegram and Slack simultaneously.

```
[source: contact_form] → [parallel_split] → [target: telegram]
                                           → [target: slack]
                                → [parallel_join] → [target: log]
```

#### Create the pipeline (CLI)

```bash
#!/bin/bash
# example1-contact-to-telegram-slack.sh
set -euo pipefail

BASE="http://localhost:8080/api/v1"
AUTH="Authorization: Bearer $(stacker token)"
CT="Content-Type: application/json"

# 1. Create template
TEMPLATE=$(curl -sf -X POST "$BASE/pipes/templates" \
  -H "$AUTH" -H "$CT" \
  -d '{"name":"Contact Form → Telegram + Slack","description":"Forward contact form submissions to Telegram and Slack"}' \
  | jq -r '.item.id')
echo "Template: $TEMPLATE"

DAG="$BASE/pipes/$TEMPLATE/dag"
add_step() { curl -sf -X POST "$DAG/steps" -H "$AUTH" -H "$CT" -d "$1" | jq -r '.item.id'; }
add_edge() { curl -sf -X POST "$DAG/edges" -H "$AUTH" -H "$CT" -d "$1" > /dev/null; }

# 2. Add steps
SOURCE=$(add_step '{
  "name": "contact_form",
  "step_type": "source",
  "step_order": 1,
  "config": {
    "url": "http://website:3000/api/contact",
    "method": "POST"
  }
}')

SPLIT=$(add_step '{
  "name": "fan_out",
  "step_type": "parallel_split",
  "step_order": 2,
  "config": {}
}')

TELEGRAM=$(add_step '{
  "name": "telegram_notify",
  "step_type": "target",
  "step_order": 3,
  "config": {
    "url": "https://api.telegram.org/bot<BOT_TOKEN>/sendMessage",
    "method": "POST",
    "headers": {"Content-Type": "application/json"},
    "body_template": {
      "chat_id": "<CHAT_ID>",
      "text": "📬 New contact from {{name}} ({{email}}): {{message}}"
    }
  }
}')

SLACK=$(add_step '{
  "name": "slack_notify",
  "step_type": "target",
  "step_order": 3,
  "config": {
    "url": "https://hooks.slack.com/services/T.../B.../xxx",
    "method": "POST",
    "headers": {"Content-Type": "application/json"},
    "body_template": {
      "text": "📬 New contact form submission",
      "blocks": [
        {"type": "section", "text": {"type": "mrkdwn", "text": "*From:* {{name}} ({{email}})\n*Message:* {{message}}"}}
      ]
    }
  }
}')

JOIN=$(add_step '{"name":"merge","step_type":"parallel_join","step_order":4,"config":{}}')

LOG=$(add_step '{
  "name": "log_delivery",
  "step_type": "target",
  "step_order": 5,
  "config": {"url": "http://telegraf:8186/write", "method": "POST"}
}')

# 3. Connect edges
add_edge "{\"from_step_id\":\"$SOURCE\",\"to_step_id\":\"$SPLIT\"}"
add_edge "{\"from_step_id\":\"$SPLIT\",\"to_step_id\":\"$TELEGRAM\"}"
add_edge "{\"from_step_id\":\"$SPLIT\",\"to_step_id\":\"$SLACK\"}"
add_edge "{\"from_step_id\":\"$TELEGRAM\",\"to_step_id\":\"$JOIN\"}"
add_edge "{\"from_step_id\":\"$SLACK\",\"to_step_id\":\"$JOIN\"}"
add_edge "{\"from_step_id\":\"$JOIN\",\"to_step_id\":\"$LOG\"}"

# 4. Validate
curl -sf -X POST "$DAG/validate" -H "$AUTH" -H "$CT" | jq .

echo "✅ Pipeline ready! Template: $TEMPLATE"
```

#### Test it

```bash
# Create an instance
INSTANCE=$(curl -sf -X POST "$BASE/pipes/instances" \
  -H "$AUTH" -H "$CT" \
  -d "{\"pipe_template_id\":\"$TEMPLATE\",\"deployment_hash\":\"my-deploy\",\"name\":\"Contact notifications\"}" \
  | jq -r '.item.id')

# Trigger with sample form data
curl -sf -X POST "$BASE/pipes/instances/$INSTANCE/dag/execute" \
  -H "$AUTH" -H "$CT" \
  -d '{
    "input_data": {
      "name": "Alice Johnson",
      "email": "alice@example.com",
      "message": "I would like to learn more about your services."
    }
  }' | jq '.status, .completed_steps, .failed_steps'
# → "completed", 6, 0
```

---

### Example 2: Contact Form → PostgreSQL → CDC → Telegram

A more realistic flow: the website saves form data to PostgreSQL, and the CDC source detects the new row and triggers a Telegram notification. This decouples the website from the notification system.

```
[cdc_source: pg_contacts] → [transform: format_message] → [target: telegram]
```

The website writes to PostgreSQL normally — no changes needed. The pipeline watches for new rows.

#### PostgreSQL setup (one-time)

```sql
-- Enable logical replication (postgresql.conf: wal_level = logical)
-- Create a publication for the contacts table
CREATE PUBLICATION contact_pub FOR TABLE public.contacts;
-- Stacker will create the replication slot automatically
```

#### Create the pipeline (CLI)

```bash
#!/bin/bash
# example2-cdc-contact-to-telegram.sh
set -euo pipefail

BASE="http://localhost:8080/api/v1"
AUTH="Authorization: Bearer $(stacker token)"
CT="Content-Type: application/json"

TEMPLATE=$(curl -sf -X POST "$BASE/pipes/templates" \
  -H "$AUTH" -H "$CT" \
  -d '{"name":"CDC Contact → Telegram","description":"Watch PostgreSQL contacts table, notify via Telegram"}' \
  | jq -r '.item.id')

DAG="$BASE/pipes/$TEMPLATE/dag"
add_step() { curl -sf -X POST "$DAG/steps" -H "$AUTH" -H "$CT" -d "$1" | jq -r '.item.id'; }
add_edge() { curl -sf -X POST "$DAG/edges" -H "$AUTH" -H "$CT" -d "$1" > /dev/null; }

# Step 1: CDC source — watch the contacts table
CDC=$(add_step '{
  "name": "pg_contacts",
  "step_type": "cdc_source",
  "step_order": 1,
  "config": {
    "replication_slot": "contacts_pipe_slot",
    "publication": "contact_pub",
    "tables": ["public.contacts"]
  }
}')

# Step 2: Transform — format CDC event into a readable message
TRANSFORM=$(add_step '{
  "name": "format_message",
  "step_type": "transform",
  "step_order": 2,
  "config": {
    "field_mapping": {
      "chat_id": "<YOUR_CHAT_ID>",
      "text": "📬 New contact form!\n\nName: $.after.name\nEmail: $.after.email\nMessage: $.after.message\nSubmitted: $.captured_at"
    }
  }
}')

# Step 3: Target — send to Telegram Bot API
TELEGRAM=$(add_step '{
  "name": "telegram",
  "step_type": "target",
  "step_order": 3,
  "config": {
    "url": "https://api.telegram.org/bot<BOT_TOKEN>/sendMessage",
    "method": "POST",
    "headers": {"Content-Type": "application/json"}
  }
}')

# Connect: CDC → Transform → Telegram
add_edge "{\"from_step_id\":\"$CDC\",\"to_step_id\":\"$TRANSFORM\"}"
add_edge "{\"from_step_id\":\"$TRANSFORM\",\"to_step_id\":\"$TELEGRAM\"}"

# Validate
curl -sf -X POST "$DAG/validate" -H "$AUTH" -H "$CT" | jq .

echo "✅ Pipeline ready! Template: $TEMPLATE"
echo ""
echo "Activate with: stacker pipe activate <INSTANCE_ID> --trigger webhook"
```

#### Test with simulated CDC event

```bash
INSTANCE=$(curl -sf -X POST "$BASE/pipes/instances" \
  -H "$AUTH" -H "$CT" \
  -d "{\"pipe_template_id\":\"$TEMPLATE\",\"deployment_hash\":\"my-deploy\",\"name\":\"Contact CDC\"}" \
  | jq -r '.item.id')

# Simulate a CDC INSERT event (in production this comes from PostgreSQL WAL)
curl -sf -X POST "$BASE/pipes/instances/$INSTANCE/dag/execute" \
  -H "$AUTH" -H "$CT" \
  -d '{
    "input_data": {
      "table_name": "contacts",
      "operation": "INSERT",
      "after": {
        "id": 42,
        "name": "Bob Smith",
        "email": "bob@example.com",
        "message": "Hi, I need help with deployment."
      },
      "captured_at": "2026-04-16T13:00:00Z"
    }
  }' | jq '.status, .completed_steps'
# → "completed", 3

# For continuous listening, activate the pipe:
stacker pipe activate $INSTANCE --trigger webhook
```

---

### Example 3: Contact Form → Email + Slack

Direct webhook-based pipeline: when a form is submitted, simultaneously send a confirmation email and post to a Slack channel.

```
[source: form_webhook] → [parallel_split] → [target: email_service]
                                           → [target: slack]
                              → [parallel_join]
```

#### Create the pipeline (CLI)

```bash
#!/bin/bash
# example3-contact-to-email-slack.sh
set -euo pipefail

BASE="http://localhost:8080/api/v1"
AUTH="Authorization: Bearer $(stacker token)"
CT="Content-Type: application/json"

TEMPLATE=$(curl -sf -X POST "$BASE/pipes/templates" \
  -H "$AUTH" -H "$CT" \
  -d '{"name":"Contact Form → Email + Slack","description":"Send confirmation email and Slack notification on form submit"}' \
  | jq -r '.item.id')

DAG="$BASE/pipes/$TEMPLATE/dag"
add_step() { curl -sf -X POST "$DAG/steps" -H "$AUTH" -H "$CT" -d "$1" | jq -r '.item.id'; }
add_edge() { curl -sf -X POST "$DAG/edges" -H "$AUTH" -H "$CT" -d "$1" > /dev/null; }

# Step 1: Source — incoming form webhook
SOURCE=$(add_step '{
  "name": "form_webhook",
  "step_type": "source",
  "step_order": 1,
  "config": {
    "url": "http://website:3000/api/contact",
    "method": "POST"
  }
}')

# Step 2: Fan out
SPLIT=$(add_step '{"name":"fan_out","step_type":"parallel_split","step_order":2,"config":{}}')

# Step 3a: Email — send via Mailjet / SendGrid / any SMTP API
EMAIL=$(add_step '{
  "name": "send_email",
  "step_type": "target",
  "step_order": 3,
  "config": {
    "url": "http://notify-service:4500/api/send",
    "method": "POST",
    "headers": {"Content-Type": "application/json"},
    "body_template": {
      "to": "{{email}}",
      "subject": "Thanks for contacting us, {{name}}!",
      "body": "Hi {{name}},\n\nWe received your message and will get back to you within 24 hours.\n\nBest regards,\nThe Team"
    }
  }
}')

# Step 3b: Slack — post to #contacts channel
SLACK=$(add_step '{
  "name": "slack_post",
  "step_type": "target",
  "step_order": 3,
  "config": {
    "url": "https://hooks.slack.com/services/T.../B.../xxx",
    "method": "POST",
    "headers": {"Content-Type": "application/json"},
    "body_template": {
      "text": "📬 *New contact form*\n• Name: {{name}}\n• Email: {{email}}\n• Message: {{message}}"
    }
  }
}')

# Step 4: Merge
JOIN=$(add_step '{"name":"merge","step_type":"parallel_join","step_order":4,"config":{}}')

# Connect
add_edge "{\"from_step_id\":\"$SOURCE\",\"to_step_id\":\"$SPLIT\"}"
add_edge "{\"from_step_id\":\"$SPLIT\",\"to_step_id\":\"$EMAIL\"}"
add_edge "{\"from_step_id\":\"$SPLIT\",\"to_step_id\":\"$SLACK\"}"
add_edge "{\"from_step_id\":\"$EMAIL\",\"to_step_id\":\"$JOIN\"}"
add_edge "{\"from_step_id\":\"$SLACK\",\"to_step_id\":\"$JOIN\"}"

# Validate
curl -sf -X POST "$DAG/validate" -H "$AUTH" -H "$CT" | jq .

echo "✅ Pipeline ready! Template: $TEMPLATE"
```

#### Test it

```bash
INSTANCE=$(curl -sf -X POST "$BASE/pipes/instances" \
  -H "$AUTH" -H "$CT" \
  -d "{\"pipe_template_id\":\"$TEMPLATE\",\"deployment_hash\":\"my-deploy\",\"name\":\"Contact email+slack\"}" \
  | jq -r '.item.id')

curl -sf -X POST "$BASE/pipes/instances/$INSTANCE/dag/execute" \
  -H "$AUTH" -H "$CT" \
  -d '{
    "input_data": {
      "name": "Carol Lee",
      "email": "carol@example.com",
      "message": "Can I get a demo of the platform?"
    }
  }' | jq '.status, .completed_steps'
# → "completed", 5
```

#### Or use the `stacker pipe` shorthand

```bash
# Instead of raw curl, you can also:
stacker pipe trigger $INSTANCE --data '{"name":"Carol","email":"carol@example.com","message":"Demo please"}'

# View history
stacker pipe history $INSTANCE

# Activate for continuous operation (webhook-triggered)
stacker pipe activate $INSTANCE --trigger webhook
```

---

## Visual DAG Editor (Web)

All three examples above can be built visually — no terminal needed.

### Access

```
http://localhost:8080/editor
```

> **Demo Mode**: Works without authentication for experimenting. API calls are skipped and changes exist only in the browser. Click **"Sign Up / Login"** to persist pipelines.

### Quick start — building Example 1 visually

1. **Open the editor** at `http://localhost:8080/editor`

2. **Drag steps from the palette** (left sidebar):
   - Drag **"Source"** 📥 onto the canvas → click it → set name: `contact_form`
   - Drag **"Parallel Split"** ⑃
   - Drag two **"Target"** 📤 nodes → name them `telegram` and `slack`, configure URLs
   - Drag **"Parallel Join"** ⑂

3. **Connect steps**: Click an output handle (right side of node) → drag to the input handle (left side of next node)

4. **Configure each step**: Click any node to open the config panel on the right. Fill in URL, method, headers, etc.

5. **Delete a connection**: Select an edge → press **Delete** or **Backspace**

6. **Validate**: Click **"Validate"** → green toast = valid ✅

7. **Execute**: Click **"Execute"** → runs the pipeline with test data

### Use a starter template

Click **"Use Template"** to start from a pre-built pipeline:
- **ETL Pipeline** — source → transform → target (simplest)
- **Webhook Router** — source → condition → two targets
- **CDC Replicator** — CDC source → transform → target

### Keyboard shortcuts

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
