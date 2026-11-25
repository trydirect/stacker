# Notifications — delivery channels (Slack / Discord / Telegram / email / webhook)

Status: WS-N complete (2026-11-24). N1 ✅ schema, N2 ✅ REST/adapters, N3 ✅ event
bus, N4 ✅ email/`container.down`/CLI. Tests: notification-core 10/10 (~10ms),
`notification_channels` 9/9, lib green. Remaining hardening (non-blocking):
SSRF block for private ranges, admin-harness test for global channels,
email integration against a live notify service.

Plan: `config/PLAN-notifications.md`.
Contracts (source of truth):
- `config/shared-fixtures/api-contracts/notifications-channels.json`
- `config/shared-fixtures/api-contracts/notifications-events.json`
BDD scenarios: `config/shared-fixtures/features/notifications.feature`.

## Model

Module layout (2026-11-24): pure logic lives in the leaf crate
`crates/notification-core` (events, channel payloads, masking, redaction,
retry/backoff, HTTP delivery) — its unit tests run without building the
monolith; the glue (Vault, db layer, routes) stays in
`src/services/notification.rs` and `src/routes/notification/`. The same
principle applies to the following workstreams: pure logic in `crates/`,
integration in the monolith.

- `notification_channel` — a channel: per-project or a global admin channel
  (`project_id IS NULL`, decision N1 of 2026-11-24). Types: `slack`, `discord`,
  `telegram`, `email`, `webhook`. Only users with the `root`/`group_admin` role
  can create a global channel. Secrets (webhook URL, bot token) are stored
  **only in Vault** under `vault_path` and never returned by API/logs/MCP —
  responses carry just a `targetHint` (masked address).
- `notification_dispatch` — a delivery attempt: `status`
  (`pending`/`sent`/`failed`/`skipped`), `attempts`, `last_error`
  (secrets are redacted before recording).

## Events (edge-triggered)

`deploy.succeeded`, `deploy.failed`, `backup.succeeded`, `backup.failed`,
`preview.ready`, `preview.failed`, `container.down`, `channel.test`.

One notification per state transition, not per poll. Delivery is at-least-once
with retry/backoff on 429/5xx. A channel's `events` filter is an allow-list;
an empty list means all events. **Project events go only to that project's
channels**; global admin channels are reserved for organization-level events
(guard against cross-tenant leakage, code-review B1).

## Relations to other workstreams

- WS-1 (backups, `config/PLAN-backups.md`): `backup.succeeded/failed` events.
- WS-G (GitHub, `config/PLAN-github-webhooks.md`): `preview.ready/failed` are
  additionally delivered as PR comments through the GitHub App (not through the
  channel adapters).
- The existing webhook target `stacker monitor`
  (`src/console/commands/cli/monitor.rs:63-92`) connects to the same bus as the
  `container.down` event.
- Email goes through the existing RabbitMQ → notify channel
  (`src/services/project_cleanup_notifier.rs`).

## Project rules

- New migrations only; run `cargo sqlx prepare` after changing sqlx queries.
- New endpoints need `api-reviewer` review + a casbin migration
  (routes under `/api/v1/notifications/*`).
- Secrets live only in Vault (`src/services/vault_service.rs`).
