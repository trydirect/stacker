# TODO — 1-Click Deploy via TryDirect (stacker side)

Branch: `feature/immutable-deploy` (worktree `.claude/worktrees/immutable-deploy`).
Full plan: `docs/ONE_CLICK_DEPLOY.md`.

## 1. Public validate endpoint — `POST /api/v1/deploy/validate`
- [x] Create `src/routes/oneclick_deploy/mod.rs` with `validate` handler.
- [x] Handler: read raw YAML body → `StackerConfig::from_str` → `validate_semantics`.
- [x] 200 `{valid, name, version, composition}` on success; 422 `{valid:false, errors[], warnings[]}` on invalid.
- [x] Never 500 on bad config (map parse/semantic errors to 422).
- [x] Wire route into `src/startup.rs` (public scope, like `/api/audit/*`).
- [x] Casbin migration granting `group_anonymous` POST `/api/v1/deploy/validate`.

## 2. Snapshot registry — `baked_snapshots`
- [x] Migration: `baked_snapshots` table (stack, version, provider, image_id, healthy, digests, created_at).
- [x] `src/models/baked_snapshot.rs` + `src/db/baked_snapshot.rs` (`resolve`, `record`).
- [x] Extend `src/bin/bake.rs` to persist the `BakeRecord` into the registry (via `DATABASE_URL`).
- [x] Run `cargo sqlx prepare` and commit `.sqlx/` (registry queries use runtime `query_as`, not `query_as!` — no offline data needed).

## 3. Protected clone endpoint — `POST /api/v1/deploy/clone`
- [x] Handler: registry resolve → `BootConfig` → `render_user_data` → `create_server_from_image`.
- [x] TryDirect-managed Hetzner token (`HetznerConfig::from_env()` → `HETZNER_TOKEN`).
- [x] Response `{server_id, public_ipv4, stack, provider}`; structured errors.

## 4. Verification
- [x] `cargo check --lib` / `--bins` (SQLX_OFFLINE=true)
- [x] `cargo test` (3 oneclick_deploy tests passing)
- [x] `cargo clippy` (no warnings in new code; pre-existing warnings only)
- [ ] `cargo fmt`
- [ ] curl smoke test of validate endpoint (valid + invalid YAML).

## 5. Bake (manual, needs live Hetzner token)
- [ ] Bake `ai-automation-workflows` once → snapshot → registry `image_id`.
