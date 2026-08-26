# 1-Click Deploy via TryDirect — implementation plan

Status: **in progress** (branch `feature/immutable-deploy` + dependent branches in `user/`, `blog/`).

## Goal

Seamless one-click deploy of any AI-stack from `trydirect/awesome-selfhosted-stacker`
via a Markdown badge:

```markdown
[![Deploy to TryDirect](https://try.direct/badge/deploy.svg)](https://try.direct/quick-deploy?source=github&repo=trydirect/awesome-selfhosted-stacker&path=stacker-projects/ai-automation-workflows&ref=main)
```

## Decisions (locked)

| Decision | Choice |
|---|---|
| Deploy backend | **Immutable**: clone pre-baked Hetzner snapshot (`feature/immutable-deploy`) |
| Wizard URL | `https://try.direct/quick-deploy?...` (blog Next.js page, no new vhost) |
| Backend API | **`user/`** service (prepare/start) |
| Auth | Existing blog localStorage deep-link preserve + sign-in `returnTo`; **no GitHub-OAuth gate** |
| Repo trust | **Hard allowlist** in `user/`: `repo == trydirect/awesome-selfhosted-stacker`, `path` under `stacker-projects/` |
| Secrets | Generated in **Python** in `user/` (`secrets`); repo `generate-secrets.sh` never executed server-side |
| Snapshot registry | Build registry + bake `ai-automation-workflows` once |
| stacker.yml validation | **New stacker HTTP endpoint** reusing `StackerConfig::from_str` + `validate_semantics` |

## Security model

`source=github` is informational only; browser origin cannot be verified. Trust is
enforced at the data level:

1. `source` must equal `github`.
2. `repo` must equal `trydirect/awesome-selfhosted-stacker` (MVP allowlist).
3. `path` must start with `stacker-projects/`; reject traversal (`..`); `ref` defaults to `main`.
4. Fetch server-side from `raw.githubusercontent.com/{repo}/{ref}/{path}/...` only.
5. **Never execute** the repo's `generate-secrets.sh` (prevents supply-chain RCE);
   secrets generated in Python.
6. `Referer` logged as a soft analytics signal only (`is_from_github`), no enforcement.

## Architecture

```
Badge → try.direct/quick-deploy?source=github&repo=...&path=...&ref=main   (nginx → blog:3333, no change)
        │
        ▼
blog/  /quick-deploy page ── auth guard (401 → /sign-in?returnTo=<url> → redirectToLastState)
        │  POST /api/one-click/prepare, /api/one-click/start   (blog BFF proxies to user/)
        ▼
user/  one-click_deploy module (allowlist, GitHub fetch, .env parse, secrets, Installations row)
        │  POST /api/v1/deploy/validate        POST /api/v1/deploy/clone
        ▼                                        ▼
stacker/  StackerConfig::from_str+validate_semantics   registry lookup → cloud-init → create_server_from_image
```

## stacker/ work (this repo, `feature/immutable-deploy`)

### 1. `POST /api/v1/deploy/validate` (public, rate-limited like `/api/audit/*`)
- New `src/routes/deploy/mod.rs`; wired in `src/startup.rs` via `configure`.
- Body: raw stacker.yml YAML string.
- `StackerConfig::from_str(&yaml)` (`src/cli/config_parser.rs:928`) → `validate_semantics()` (`:987`).
- 200 `{ valid, name, version, composition {app, services[]} }`; 422 `{ valid:false, errors[], warnings[] }`.
- Casbin `group_anonymous` rule (new migration, pattern `20260726120000_casbin_audit_public_rules.up.sql`).

### 2. `baked_snapshots` registry
- Migration `bake_snapshots` (stack, version, provider, image_id, healthy, digests JSONB, created_at).
- `src/db/baked_snapshot.rs`, `src/models/baked_snapshot.rs`: `resolve`/`record`.
- Extend `src/bin/bake.rs` / `src/helpers/bake.rs` to persist the `BakeRecord`.
- Run `cargo sqlx prepare` after any sqlx query change.

### 3. `POST /api/v1/deploy/clone` (protected)
- Request `{ stack, version, region, server_type, domain, admin_email, env{} }`.
- Registry resolve → `BootConfig` → `render_user_data` (`src/helpers/cloud_init.rs`) →
  `HetznerCloudConnector::create_server_from_image` (`src/connectors/hetzner.rs`).
- Token: TryDirect-managed Hetzner token from env/settings (`HETZNER_TOKEN`).
- Response `{ server_id, public_ipv4, deployment_id }`.

## user/ work

- `app/app/oneclick_deploy/` blueprint at `/deploy`:
  - `POST /deploy/prepare`: allowlist+shape → fetch stacker.yml/.env.example → call stacker validate →
    env fields (Python secrets) → `{stack, composition, env_fields, provider_options}`.
    422 `Invalid stacker.yml in target repository` on validation failure.
  - `POST /deploy/start`: auth-gated → `Installations` row → call stacker clone → `{deployment_id, server_id, public_ipv4}`.
- `ALLOWED_REDIRECT_DOMAINS` already includes `try.direct`.

## blog/ work

- `src/pages/quick-deploy.jsx` wizard (composition → env form → Hetzner region/type → start) — `/deploy` is a marketing/FAQ page, so the wizard is `/quick-deploy`.
- BFF `src/pages/api/one-click/{prepare,start}.js`.
- Sign-in `returnTo` fallback on `SignInPage.jsx` (same-origin only).
- Badge SVG at `blog/public/badge/deploy.svg`;

## config/ work

- No nginx change (try.direct default → blog; `/server/user/` → user already routed).
- Keep `shared-fixtures/immutable-deploy/boot-contract.json` in sync with cloud_init paths.

## Definition of Done

1. Badge in `ai-automation-workflows` opens console preloaded with Flowise+n8n+Ollama+Qdrant.
2. `.env.example` vars land in the env form (secrets autofilled).
3. Invalid stacker.yml → `Invalid stacker.yml in target repository` (422), never a 500.
