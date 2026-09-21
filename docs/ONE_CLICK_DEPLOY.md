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

> For how one secret value travels from the author's machine into a buyer's
> clone — and which component owns each step — see
> [SECRET_LIFECYCLE.md](SECRET_LIFECYCLE.md).

### 2. `baked_snapshots` registry
- Columns: `stack`, `version`, `provider`, `image_id`, `healthy`, `digests` JSONB,
  `created_at`, plus two added later:
  - `config_contract` JSONB (`20260911120000`) — the author's field policy pinned to
    the image, so the clone path regenerates `mutability: generated` fields per buyer
    instead of every clone inheriting the one value baked at bake time.
  - `required_env_keys` JSONB (`20260919120000`) — the `${VAR}` names the baked compose
    references. The clone path refuses a deploy whose environment cannot satisfy them,
    because Compose resolves an unsatisfied reference to an empty string with only a
    warning. NULL means the check is skipped: either the snapshot predates the column,
    or it was baked with `--allow-unsanitized-snapshot`.
- `src/db/baked_snapshot.rs`, `src/models/baked_snapshot.rs`: `resolve`/`record`.
  Both reads are `SELECT *`. `required_env_keys` carries `#[sqlx(default)]`, so it
  tolerates a database that has not run its migration yet; `config_contract` does
  **not**, so `resolve()` fails outright against a database missing that column.
- `src/bin/bake.rs` / `src/helpers/bake.rs` persist the `BakeRecord`. Before snapshotting,
  `src/helpers/bake_finalize.rs` sanitizes the build box over SSH (blank the co-located
  `.env`, parameterize secrets embedded in compose values, drop credential-bearing data
  volumes, strip SSH host keys / machine-id / cloud-init state) — hence `bake --ssh-key`.
- These three queries use runtime `sqlx::query_as`, not the compile-time macros, so they
  need no `.sqlx` entry. Run `cargo sqlx prepare` after changing any *macro* query.

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

---

## Env field types

The `POST /deploy/prepare` response includes a `type` field on each env entry so
the frontend can render the correct input widget (text, checkbox, password).

### How types are determined

Types are resolved in two stages:

1. **Declared types from `config_contract`** — when `stacker.yml` declares a
   `display` field on a `config_contract.services.<service>.fields.<FIELD>`
   entry, the user service reads it from the validate response and applies it
   to the matching `env_fields` entry. This is the **primary** source of truth.

2. **Fallback heuristic** — for fields not declared in `config_contract`, the
   user service infers `boolean` from values `true`/`false`/`yes`/`no`/`1`/`0`/`on`/`off`,
   and `password` for fields whose key matches secret-related keywords
   (`SECRET`, `PASSWORD`, `TOKEN`, etc.). Everything else defaults to `text`.

### Response shape

```json
{
  "key": "FLOCI_TLS_ENABLED",
  "value": "true",
  "required": true,
  "secret": false,
  "type": "boolean"
}
```

### Frontend rendering

| `type` | Widget | Value sent to `/deploy/start` |
|--------|--------|-------------------------------|
| `boolean` | Checkbox (toggle) | `"true"` or `"false"` (string) |
| `number` | Number input | The numeric string as-is |
| `password` | Password input (masked, with regeneration) | The string value |
| `text` | Text input | The string value |

### Declaring types in stacker.yml

Template authors declare `display` on `config_contract` fields:

```yaml
config_contract:
  services:
    app:
      fields:
        TLS_ENABLED:
          mutability: editable
          display: boolean
        PORT:
          mutability: editable
          display: number
        API_SECRET:
          mutability: generated
          type: base64
          display: password
          length: 48
```

See [FIELD_POLICY.md](./FIELD_POLICY.md) for the full `display` reference.
