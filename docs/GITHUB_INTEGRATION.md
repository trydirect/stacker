# GitHub integration — App, webhooks, watch paths, auto-deploy (WS-G)

Status: complete. G1 ✅ (2026-11-24): schema `github_installation`, `github_repo`,
`github_webhook_delivery` + casbin rule for the anonymous webhook route.
G2 ✅ (2026-11-24): receiver `POST /api/v1/github/webhook` — verifies
`X-Hub-Signature-256` (HMAC, constant-time), anti-replay via `X-GitHub-Delivery`,
event normalization. G3 ✅ (2026-11-24): watch paths (glob + `!` exclusions).
G4 ✅ (2026-11-24): Repo Connect API `/api/v1/github/repos` (CRUD, owner-gated),
installation events land in `github_installation`; the auto-deploy switch is
`github_repo.autodeploy` (it superseded the catalog `autodeploy` field).
G5 ✅ (2026-11-24): auto-deploy of matched pushes through `redeploy_saved_project`
(`src/services/redeploy.rs`) — deploys onto the existing server
(`server.server_id`), never provisions a new VM; the webhook response carries
`triggered` (queued|skipped|failed).

Plan: `config/PLAN-github-webhooks.md` (continuation of `PLAN-managed-previews.md`).
Contract: `config/shared-fixtures/api-contracts/github-webhooks.json`.
BDD scenarios: `config/shared-fixtures/features/github-autodeploy.feature`.

## Module layout

Pure logic lives in the leaf crate `crates/github-events` (signature
verification, normalization of `push`/`pull_request`/`installation`/
`installation_repositories`, watch-path matching) — its tests run without
building the monolith. The glue (Vault, db, routes) lives in
`src/routes/github/`, `src/db/github.rs`, `src/models/github.rs`.

## Model

- `github_installation` — a GitHub App installation (user_id ↔ installation_id).
- `github_repo` — a connected repository: repo, branch, `watch_paths[]`,
  `autodeploy`, linked to `project_id`.
- `github_webhook_delivery` — anti-replay: a delivery id is processed once.

## Webhook

- Authentication is `X-Hub-Signature-256` only (secret in Vault under
  `github/webhook_secret`; the casbin route is anonymous — same precedent as
  the payout webhook).
- A repeated delivery id returns `200 {duplicate: true}` without processing.
- A push matches on repo + branch + watch paths → the response carries a
  `matched` list (the seam for G5: the deploy plan is triggered from there).

## Project rules

- New migrations only; run `cargo sqlx prepare` after changing sqlx queries.
- Repo-connect REST endpoints need `api-reviewer` review + a casbin migration.
- Secrets (webhook secret, App private key) live only in Vault.
