# Agent Gateway — Developer Setup & Infrastructure HOWTO

The **agent-gateway** is a standalone, independently-scalable MCP server that
exposes two tools an AI agent cannot do itself:

- **`resolve_image(reference)`** — ground truth about a Docker image (exists,
  digest, size, architectures, tags, last_pushed, official/pinned, grade,
  optional CVE summary). No infra; safe to run anywhere.
- **`deploy_ephemeral(compose_yaml, ttl_secs)`** — provision a throwaway box,
  run the compose, return `{live_url, logs, health}`, and auto-teardown after a
  TTL. Provisions **real** (paid) cloud infra — read the guardrails below.

Architecture: the pure decision/assembly logic lives in the `agent-tools` crate
(`cargo test -p agent-tools`, no DB/cloud); heavy I/O (Docker Hub, RabbitMQ, DB,
Vault, cloud) is injected via the `ImageResolver` / `SandboxController` traits and
supplied by the gateway binary, which reuses `stacker::mcp` (JSON-RPC/WebSocket).

---

## 1. Prerequisites

Runtime services (same ones the main `server` needs):
- **PostgreSQL** — deployment/agent/server state.
- **Redis** — Docker Hub response cache (and shared rate-limit if fronted).
- **RabbitMQ** — deploy messages to the install service.
- **HashiCorp Vault** — SSH keys, agent tokens, sandbox cloud creds.
- **Install service** running and consuming `install.*` (it runs OpenTofu →
  provisions the box → `docker compose up`).
- A **managed sandbox cloud account** (Hetzner) + API token, dedicated to
  sandboxes so quotas/billing are isolated from customer deployments.

Toolchain: Rust (2021), `sqlx-cli` for migrations. Optional: `trivy` on `PATH`
for CVE summaries.

---

## 2. Configuration (environment)

Shared with the platform:
```
REDIS_URL=redis://127.0.0.1/0
DOCKERHUB_USERNAME=...            # optional; raises Docker Hub rate limits
DOCKERHUB_TOKEN=...               # optional PAT
AUDIT_TRIVY_ENABLED=1            # optional; include cve_summary in resolve_image
# Postgres / AMQP / Vault come from configuration.yaml (same as `server`).
```

Gateway-specific:
```
AGENT_GATEWAY_BIND=0.0.0.0:4600  # its own port, separate from `server`

# Managed sandbox cloud pool (TryDirect account — NOT the caller's):
SANDBOX_CLOUD_PROVIDER=hetzner
SANDBOX_HETZNER_TOKEN=...         # or a Vault path; provisions the throwaway boxes
SANDBOX_REGION=fsn1
SANDBOX_SERVER_SIZE=cpx11        # smallest usable box

# Quotas (enforced in agent-tools::sandbox before any provisioning):
SANDBOX_DEFAULT_TTL_SECS=1800    # 30m
SANDBOX_MAX_TTL_SECS=7200        # 2h
SANDBOX_MAX_CONCURRENT_PER_USER=3

# TTL reaper:
SANDBOX_REAPER_INTERVAL_SECS=60  # how often to scan for expired sandboxes
```

Agent auth: callers authenticate with a bearer token resolved to a `User` by the
reused auth middleware (JWT today; a dedicated "agent token" scope is a
follow-up). Casbin must grant the agent role access to `/mcp`.

---

## 3. Build & run

```bash
# Pure core tests — no services needed:
cargo test -p agent-tools

# Build the standalone server:
SQLX_OFFLINE=true cargo build --bin agent-gateway

# Apply migrations (adds sandbox TTL columns / casbin grant for /mcp):
sqlx migrate run

# Run it (needs Postgres/Redis/RabbitMQ/Vault + install service):
cargo run --bin agent-gateway
```

It serves `/health` and the MCP WebSocket at `/mcp` (JSON-RPC 2.0, `mcp`
subprotocol), independently of the main `server` — scale it as its own
deployment/replica set.

---

## 4. Using the tools (MCP client)

Connect an MCP client to `ws://<host>:4600/mcp` and `initialize`, then:

```jsonc
// tools/list  -> shows resolve_image + deploy_ephemeral
// tools/call resolve_image
{ "name": "resolve_image", "arguments": { "reference": "redis:7-alpine" } }
// -> { exists, official, pinned, digest, size_bytes, architectures:["amd64","arm64"],
//      recent_tags:[...], last_pushed, grade:"A", cve_summary?:{...} }

// tools/call deploy_ephemeral
{ "name": "deploy_ephemeral",
  "arguments": { "compose_yaml": "services:\n  web:\n    image: nginx:1.27-alpine\n    ports: [\"80:80\"]\n",
                 "ttl_secs": 900 } }
// -> { id:"sandbox_<uuid>", live_url, health, expires_at }   (poll status for the URL)
```

---

## 5. TTL reaper (auto-teardown)

A background task in the gateway scans every `SANDBOX_REAPER_INTERVAL_SECS` for
deployments whose `metadata.ttl_expires_at < now()` (selection logic:
`agent_tools::sandbox::select_expired`, unit-tested) and tears them down. Teardown
reuses the existing destroy path — server delete + Vault cleanup
(`src/routes/server/delete.rs`) and the install-service OpenTofu destroy — so a
sandbox never outlives its TTL even if the agent disconnects.

Operational: watch reaper logs; a stuck box is caught on the next scan. To change
aggressiveness, tune `SANDBOX_REAPER_INTERVAL_SECS` and `SANDBOX_MAX_TTL_SECS`.

---

## 6. Cost & abuse guardrails (managed pool)

Because `deploy_ephemeral` provisions **real paid infra on the TryDirect
account**, these are enforced *before* any provisioning (all in
`agent-tools::sandbox`, unit-tested):

- **TTL clamp** — requests are clamped to `[1, SANDBOX_MAX_TTL_SECS]`; auto-teardown guaranteed.
- **Per-user concurrency** — refused (`QuotaExceeded`) at `SANDBOX_MAX_CONCURRENT_PER_USER`.
- **Smallest box** — `SANDBOX_SERVER_SIZE` (e.g. `cpx11`).
- **Compose safety gate** — `gate_compose` refuses `privileged`, Docker-socket
  mounts, and host networking (reuses the `td-audit` compose parser); extend with
  `td_audit::exposure` / security checks as needed. Untrusted agent compose never
  reaches a host with an escape hatch.

Recommend: a dedicated cloud sub-account with a hard spend cap, plus alerting on
active-sandbox count.

---

## 7. v2 fast path — Kata/Firecracker warm pool

v1 provisions a fresh VM per request (~90s to a live URL). The `SandboxController`
trait lets a **warm microVM pool** drop in with no tool/test changes:

- Keep N pre-provisioned Linux hosts (KVM) running a microVM runtime — **Kata
  Containers** (Stacker's `Deployment.runtime` already supports `kata`) or
  **Firecracker/Cloud Hypervisor**. A request grabs a slot, `docker compose up`
  inside a microVM (seconds), then the slot is wiped and returned to the pool.
- This is the self-hosted equivalent of **Docker Sandboxes** (Docker Desktop's
  agent feature; local-only, no hosted API — so we reuse the *pattern*, not the
  product). MicroVM isolation is what makes running untrusted agent compose on a
  shared host safe.
- Host prerequisites: KVM enabled, `kata-runtime` (or Firecracker) installed, a
  pool manager tracking slot lease/reset. Security-review the isolation boundary
  before exposing to untrusted callers.

Implement as `KataPoolController: SandboxController` and select it via config; the
MCP tools and their tests are unchanged.
