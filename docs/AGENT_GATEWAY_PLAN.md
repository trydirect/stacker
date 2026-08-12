# Plan: Agent-facing MCP gateway — `resolve_image` + `deploy_ephemeral`

## Context

Human-facing "checkers" are commodity — any LLM can lint a compose in-chat. The
durable wedge is the set of things an AI agent **structurally cannot do itself**:
get **ground truth** (it hallucinates image refs — we literally spent a day on a
hallucinated `trydirect/redis`) and **execute** real infrastructure (it can
reason about a stack but can't run it). TryDirect owns both.

Ship two MCP tools that give an agent *eyes* and *hands*:
- **`resolve_image(ref)`** → ground truth about a Docker image: exists, digest,
  size, architectures, tags, last_pushed, official/pinned, optional CVE summary.
- **`deploy_ephemeral(compose, ttl)`** → provision a throwaway box, run the
  compose, return `{live_url, logs, health}`, auto-teardown after a TTL.

They must run as **their own scalable actix server** (separate process/replicas,
own port), isolated from the main API, since agent WS traffic + long-running
deploys shouldn't contend with the core platform.

### Confirmed decisions
- **Gateway = new binary reusing the `stacker` lib.** A new `[[bin]]
  agent-gateway` imports `stacker::mcp` (ToolRegistry, JSON-RPC protocol, WS
  actor) + connectors/db/MqManager, registers only the two agent tools, runs its
  own `HttpServer`. Independently scalable at runtime; the ToolHandler structs
  live in `src/mcp/tools/` as requested.
- **Sandbox infra = TryDirect-managed cloud pool + quotas.** The agent supplies
  nothing; TryDirect provisions on its own Hetzner account under strict quotas
  (TTL default 30m / max 2h, smallest box, max N concurrent sandboxes per user).
  Cloud backend behind a trait so bring-your-own is possible later.
- **Substrate = fresh VM v1 behind a `SandboxController` trait; Kata/Firecracker
  warm-pool as v2.** v1 reuses the proven OpenTofu→Hetzner deploy path (~90s to a
  live URL). Docker Sandboxes (Docker Desktop 4.60+, custom microVM VMM, `sbx`
  CLI) is **local-only with no hosted API**, so it can't be called as a service —
  but its microVM-per-sandbox model is the v2 target, self-hosted via **Kata**
  (Stacker's `Deployment.runtime` already carries `runc`|`kata`) or Firecracker.
  The trait keeps the tools/tests unchanged when the fast path drops in.

## Architecture: pure core + trait-injected heavy deps + standalone binary

Unlike `td-audit`, these tools inherently need the platform (Docker Hub, MQ, DB,
Vault). So we keep the **testable core pure** and inject the heavy operations via
traits (mockable), then wrap them as MCP `ToolHandler`s and serve from a new
binary.

```
crates/agent-tools/                 # pure, no actix/sqlx (deps: serde, thiserror,
  src/lib.rs                        #   async-trait, td-audit)
    image.rs      # ResolvedImage DTO; `trait ImageResolver`; assemble/normalize
                  #   ground-truth + reuse td_audit::image::audit_image for a grade
    sandbox.rs    # SandboxSpec/Handle/Status DTOs; `trait SandboxController`
                  #   { launch, status, teardown }; TTL math; quota decision;
                  #   reaper "which are expired" selection; compose safety gate
    error.rs
src/mcp/tools/
  resolve_image.rs       # ToolHandler: parse args -> real ImageResolver -> ToolContent
  deploy_ephemeral.rs    # ToolHandler: validate+quota -> real SandboxController
src/bin/agent-gateway.rs # own HttpServer: /mcp (WS) + /health; focused ToolRegistry
```

Each `ToolHandler` builds its real trait impl from `ToolContext { user, pg_pool,
settings }` (+ app_data the gateway provides), exactly like existing tools.

## Tool 1 — `resolve_image` (ground truth)

Reuse, don't reinvent:
- **`src/connectors/dockerhub_service.rs`** `DockerHubConnector` (+ `MockDockerHubConnector`
  for tests) → existence, `TagSummary.digest`/`last_updated`, tag list; Redis-cached.
- **`src/helpers/dockerhub.rs`** `Image { architecture, os, size, digest, last_pushed }`
  via `GET /v2/repositories/{ns}/{repo}/tags/{tag}/images` → the arch[]/size the
  connector trait doesn't expose yet. Wire this into the real `ImageResolver`.
- **`src/routes/audit/mod.rs`** `DockerHubMetadata::fetch` + `days_since_iso8601` +
  the `library/` normalization / official / pinned logic — lift into the pure
  crate (they're already almost pure).
- **`td_audit::image`** `audit_image()` + `parse_trivy_report()` → include a grade
  and optional CVE summary (Trivy opt-in via `AUDIT_TRIVY_ENABLED`, as today).

Result DTO: `{ reference, exists, official, pinned, digest, size_bytes,
architectures[], recent_tags[], last_pushed, grade, cve_summary? }`. Real impl is
network-backed; pure assembly + normalization is unit-tested with a mock resolver.

## Tool 2 — `deploy_ephemeral` (hands, with a TTL reaper)

`SandboxController` trait: `launch(SandboxSpec) -> Handle`, `status(&Handle) ->
{health, live_url, log_excerpt}`, `teardown(&Handle)`.

**`FreshVmController` (v1)** reuses the existing deploy machinery:
- Build a minimal **`Payload`** (`src/forms/project/payload.rs`) with a
  gzipped compose, `deployment_hash = "sandbox_{uuid}"`, managed-account cloud
  creds (from config/Vault), smallest Hetzner box.
- Publish via **`MqManager`** / **`InstallServiceClient::deploy()`**
  (`install.start.tfa.all.all`) — same path as normal deploys.
- Poll status/health via **`db::deployment::fetch_by_deployment_hash`** + agent
  heartbeat; `live_url` from `server.srv_ip`; logs via the deployment **events
  feed** (`src/routes/deployment/events.rs`).
- **Teardown** reuses the destroy path (server delete + Vault cleanup,
  `src/routes/server/delete.rs`, install-service `on_fail` destroy).

**TTL reaper (new — the one real gap):** a background tokio task in the gateway
that periodically selects deployments with `metadata.ttl_expires_at < now()` and
tears them down. Selection logic is pure/tested; the effecting call reuses
teardown. (If the agent/cron mechanism is a better host, use it; otherwise a
simple interval task in the gateway.)

**Guards (managed-pool safety):** clamp TTL to max; enforce max concurrent
sandboxes/user (count via db); smallest size; **gate the compose through
`td-audit`** (`audit_exposure` + security) and refuse `privileged`, host mounts,
etc. before launch — a natural reuse that stops the sandbox from being an abuse
vector.

## MCP wiring & the standalone server

- Reuse `stacker::mcp` wholesale: `ToolHandler`/`ToolRegistry`
  (`src/mcp/registry.rs`), JSON-RPC types (`src/mcp/protocol.rs`), the WS actor
  (`src/mcp/websocket.rs`). Add the two tools to the registry the gateway builds.
- `src/bin/agent-gateway.rs`: mirror `src/bin/server.rs` boot, but a lean
  `HttpServer` exposing only `/mcp` (WS) + `/health`, its own port/config, its own
  deployable. Reuse the existing auth middleware for agent callers (token →
  `ReqData<User>`); an "agent token" scope is a follow-up.
- Register as a `[[bin]]` in `Cargo.toml`; add `crates/agent-tools` to
  `[workspace] members`.

## TDD / BDD (mirrors the td-audit discipline)

- **Unit (`cargo test -p agent-tools`)** — pure, no DB/actix:
  - `resolve_image`: assemble ground-truth from a **mock `ImageResolver`**;
    ref-normalization cases (`redis` → `library/redis`, tag vs digest, `ghcr.io/...`),
    official/pinned, grade mapping.
  - `sandbox`: TTL clamp, quota decision (allow/deny at N), reaper expired-set
    selection, compose safety-gate refusals — all against a **mock
    `SandboxController`**.
- **BDD (cucumber, matching `tests/features/mcp.feature`)** — a
  `tests/features/agent_gateway.feature` driving the MCP JSON-RPC over WS with
  mock controllers: `tools/list` shows both tools; `resolve_image` returns
  ground-truth JSON; `deploy_ephemeral` returns a handle + live_url and is
  refused when over quota / when the compose is unsafe.
- Fixtures (compose samples, a captured Docker Hub `/images` response, a Trivy
  sample) go in **`config/shared-fixtures/agent/`** (canonical) with portable
  copies in the crate, per the shared-fixtures convention.

## Workflow, deliverables & developer HOWTO

- **Isolated git worktree.** All work happens in a dedicated worktree/branch
  (`feature/agent-gateway`), like the audit work — never on `dev` directly.
- **TDD throughout.** Every unit is RED→GREEN: write the failing test (pure core
  with mock `ImageResolver`/`SandboxController`) first, implement, run
  `cargo test -p agent-tools`; BDD scenarios likewise before wiring.
- **Plan + HOWTO checked in.** Copy this plan into the repo and write a
  developer-facing setup guide **`docs/AGENT_GATEWAY_SETUP.md`** so anyone can
  stand up the infrastructure. It must cover:
  - **Prereqs**: Rust toolchain; Postgres, Redis, RabbitMQ, Vault, and the
    install service running; a **managed sandbox cloud account** (Hetzner) + token.
  - **Env/config**: `REDIS_URL`, AMQP/MQ settings, `VAULT_*`, `DOCKERHUB_*`,
    `AUDIT_TRIVY_ENABLED`; sandbox managed-account creds (e.g.
    `SANDBOX_HETZNER_TOKEN`, region/size); quotas (`SANDBOX_DEFAULT_TTL=30m`,
    `SANDBOX_MAX_TTL=2h`, `SANDBOX_MAX_CONCURRENT_PER_USER`); gateway bind port;
    agent-auth token setup.
  - **Run**: build + run `agent-gateway`; connect an MCP client and call
    `tools/list` / `resolve_image` / `deploy_ephemeral`; run the test suites.
  - **TTL reaper**: how it selects + tears down expired sandboxes, and how to
    tune the interval; how teardown maps to server-delete + Vault cleanup.
  - **Cost guardrails**: quotas, smallest box, auto-teardown, the td-audit
    compose safety-gate — the knobs that keep a managed pool from being abused.
  - **v2 fast path (Kata/Firecracker)**: host prerequisites (KVM, kata-runtime /
    Firecracker), how it slots behind `SandboxController`, and the security notes
    for running untrusted agent compose in microVMs (the self-hosted Docker
    Sandboxes model).

## Milestones

- **M0** — `crates/agent-tools` scaffold (DTOs, `ImageResolver`/`SandboxController`
  traits, error), workspace member, `agent-gateway` binary booting a `/mcp` + `/health`
  server with an empty registry. TDD skeleton.
- **M1** — `resolve_image`: pure assembly + mock tests, real DockerHub-backed
  resolver (wire arch/size endpoint), ToolHandler, register, BDD. Ship — pure
  ground-truth, low risk, no infra spend.
- **M2** — `deploy_ephemeral` core: `SandboxController` trait, `FreshVmController`
  over Payload/MqManager, quotas + compose safety-gate (reuse td-audit),
  ToolHandler, BDD with a mock controller.
- **M3** — TTL reaper + real teardown wiring + managed-account cloud creds;
  end-to-end launch→live_url→auto-teardown against a real (throwaway) box.
- **M4 (later)** — `KataPoolController` fast path behind the same trait; metrics
  (reuse the prometheus registry): `agent_sandbox_total{state}`,
  `agent_image_resolve_total{result}`.

## Critical files

- New: `crates/agent-tools/**`, `src/mcp/tools/{resolve_image,deploy_ephemeral}.rs`,
  `src/bin/agent-gateway.rs`, `config/shared-fixtures/agent/**`,
  `tests/features/agent_gateway.feature` (+ steps).
- Modified: `Cargo.toml` (workspace member + `[[bin]]` + `agent-tools` dep),
  `src/mcp/mod.rs`/`registry.rs` (export the two tools), reuse (not modify)
  `dockerhub_service.rs`, `helpers/dockerhub.rs`, `forms/project/payload.rs`,
  `helpers/mq_manager.rs`, `connectors/install_service/client.rs`,
  `db/deployment.rs`, `routes/server/delete.rs`, `routes/deployment/events.rs`,
  `td-audit`.

## Verification

- `cargo test -p agent-tools` — pure unit tests with mock `ImageResolver` /
  `SandboxController` (no DB, no cloud).
- `cargo test -p stacker --test bdd` (or the gateway's cucumber target) — the
  `agent_gateway.feature` scenarios with mock controllers.
- `SQLX_OFFLINE=true cargo build --bin agent-gateway` — the standalone server
  compiles against the reused lib.
- Manual: run `agent-gateway`; connect an MCP client (tokio-tungstenite, like
  `tests/steps/mcp.rs`); `tools/list` → both tools; `resolve_image {"ref":"nginx:latest"}`
  → ground-truth JSON (exists/arch/size/digest/grade); `deploy_ephemeral` with a
  tiny compose → `{live_url, logs, health}`, then confirm the reaper tears it down
  after the TTL (server + Vault cleaned).
- Cost/safety checks: over-quota call → refused; `privileged` compose → refused by
  the td-audit gate before any provisioning.
