# Marketplace field policy (secret regeneration + installer-editable fields)

> Status: planned, not yet implemented. Tracking branch: `feature/marketplace-field-policy`.

## Context

Stacks built with the `stacker` CLI are published to the marketplace so other users can install/reuse them. Today, the literal secret values that were correct for the *author's own* deployment (e.g. Supabase's `ANON_KEY`, `JWT_SECRET`, `DASHBOARD_PASSWORD`, `POSTGRES_PASSWORD`) get copied byte-for-byte into every install of the template. Every buyer of a template currently gets the exact same secrets as the original author and as every other buyer — a real security hole once a template is bought/reused by multiple parties.

Investigation confirmed:
- `config_contract.services.<name>.secrets` in `stacker.yml` is **purely descriptive** today — nothing reads it to redact or regenerate anything, and it never survives past the CLI (zero references in the marketplace/webhook code).
- Publish (`src/routes/marketplace/creator.rs`) accepts `stack_definition` verbatim; the only safeguard is a manual `confirm_no_secrets: Option<bool>` checkbox (`creator.rs:296-309`) that is never checked against actual content.
- Install (`tools/v1/additional/custom_stack_mapper.py`, cached-compose fast path, lines ~606-710) writes the cached `stack_definition["compose"]` **byte-for-byte** to the final compose file. No per-installer substitution exists anywhere.
- `StackTemplateVersion` (`src/models/marketplace.rs:87-106`) is per-version, so any new persisted fields belong there.
- The existing `stacker secrets` engine (remote mode, `src/db/remote_secret.rs`, `src/routes/project|server/secret.rs`) already stores/redacts per-project secrets and should be reused so an installer's freshly generated values show up via `stacker secrets list`.
- The CLI already has a policy-free version of this exact mechanism: `is_secret_env_key()` + `generate_secrets_script_content()` (`src/console/commands/cli/init.rs:1910-1991`) emits a `scripts/generate-secrets.sh` that fills any empty, secret-looking `.env` key with `openssl rand -hex 16` — flat, no per-field type/length, no derived-value support.

**Decision (user-directed): skip automatic secret-value detection.** Field-value shape sniffing (is this hex? base64? a JWT?) is unreliable and out of scope. Instead, secret **policy is author-declared** in `config_contract` — the person who built the stack knows that `ANON_KEY` is a JWT signed by `JWT_SECRET`, that `DASHBOARD_PASSWORD` needs to be a 20+ char password, etc. `stacker config suggest-contract` (already exists, `src/cli/config_contract.rs`) and/or AI assistance can help an author draft this, but the declared contract — not a detector — is what's authoritative and used at publish/install time.

**Decision: the manifest lives in `stacker.yml`** (as an extension of `config_contract`), not as a separate backend-invented schema. Rationale: authored by the person who understands the app; versioned with the stack in git; validated by `stacker config validate`/`stacker config fix` before publish; and the *same* schema drives both local `stacker init`'s `generate-secrets.sh` and marketplace install-time generation — one source of truth instead of two.

**OSS precedent considered, not adopted wholesale:** Helm's Sprig template functions (`randAlphaNum`, `genPrivateKey`) + `lookup`-based idempotency is the closest conceptual analog for "generate per-install, don't regenerate on redeploy." Ansible's `password` lookup plugin (generate-once-and-persist-to-a-file) is directly relevant since Install Service already runs Ansible for app deployment, and its idempotency pattern is worth borrowing for redeploy-safety. Terraform's `random_password`/`tls_private_key` resources are a similar fit if generation happens in the Terraform layer instead. None of these handle the *derived* case (sign a JWT with another freshly generated secret) — that stays custom, consistent with it being app-specific knowledge. Net call: extend the existing bespoke `generate_secrets_script_content` mechanism (Rust) and mirror its policy-driven logic in Python for Install Service, rather than pulling in a new framework/dependency.

**Cross-service contract: `~/work/try.direct/config/shared-fixtures`.** This repo is the platform's existing established pattern for exactly this problem — a polyglot (Rust + Python + JS) system where field-shape mismatches between services are otherwise only caught at runtime. It already has a directly analogous precedent: `runtime-env-contract.json` (canonical JSON with `_owner`/`_consumers`/`_notes`/`version` header) is what `src/services/env_contract.rs`'s `RuntimeEnvContractResponse` mirrors today, and `immutable-deploy/boot-contract.json` documents an owner/consumer table plus an explicit "breaking change rule" for a similarly cross-service (DEPLOY/BAKE) contract. This feature's field-policy schema must follow the same convention rather than being invented ad hoc per service:
- Add `shared-fixtures/api-contracts/marketplace-field-policy.json` — canonical JSON Schema for the `config_contract.services.*.fields` policy shape: `mutability` (`fixed`/`editable`/`generated`) plus, for `generated`, the `hex`/`base64`/`alphanumeric`/`uuid`/`derived_jwt` type variants from section 1 below — with `_owner: "stacker"` and `_consumers: ["stacker", "user", "install", "tools"]` (plus the Stack Builder frontend once it's in scope), following the header convention seen in `runtime-env-contract.json`.
- Update `shared-fixtures/README.md`'s "Adding a New Contract" section and its directory-structure listing to include this file, per the repo's own documented process.
- Every implementation step below that touches this schema (Rust parser, Python generator, project API) must validate against this one shared JSON Schema in its own tests, not a locally re-derived shape — this is the enforcement mechanism the `shared-fixtures` repo exists for (see its README: "All three bugs found on 2026-04-24 would have been caught at test time with these contracts").

**TDD is mandatory for this feature.** For every unit below (Rust parser/validator, Python generator, API route), the sequence is: (1) write the failing test — including a contract-conformance test that loads `shared-fixtures/api-contracts/marketplace-field-policy.json` and asserts the implementation's (de)serialization matches it — (2) confirm it fails for the right reason, (3) implement the minimum to pass, (4) refactor. Do not write implementation code before its test exists. This applies across both the Rust (`stacker`) and Python (`tools`/`install`) sides.

## Setup (before any code)

Create a feature branch off `dev` in each repo touched, before writing any test or code:
- `stacker`: `feature/marketplace-field-policy` (created).
- `~/work/try.direct/config` (for the new `shared-fixtures` contract file): same branch-off-`dev` pattern if that repo also uses a `dev` integration branch — verify at implementation start.
- `tools`/`install` and `user`: branch off their own `dev` (or default branch, verify each repo's convention — don't assume `dev` exists in all four) only once the corresponding install-time/API steps are actually reached, not upfront in repos untouched by earlier steps.

## Design

### 1. Extend `config_contract` schema (stacker.yml) — unified field policy, not secrets-only

Today: `TargetConfigContract { required: Vec<String>, optional: Vec<String>, secret: Vec<String> }` (`src/cli/config_parser.rs:883-897`) — three separate plain name lists, no per-field behavior.

**Scope decision:** cover *all* fields, not just secrets. The reason is a third axis that both secret and non-secret fields share — **mutability**: who controls the field's final value at install time.
- `fixed` — author's value is baked in; not exposed to the installer at all.
- `editable` — author's value is only a *default*; the installer's form/env override can change it (e.g. `LOG_LEVEL` author-defaults to `warning`, buyer picks `debug`).
- `generated` — the system produces a fresh value per install; the installer never enters it directly (the secret-regeneration case from the original problem).

Unify the three old lists into one `fields` map per service:

```yaml
config_contract:
  services:
    auth:
      fields:
        POSTGRES_HOST: { mutability: fixed, required: true }
        LOG_LEVEL: { mutability: editable, required: false, type: enum, values: [debug, info, warn, error] }
        JWT_SECRET: { mutability: generated, required: true, type: hex, length: 32 }
        DASHBOARD_PASSWORD: { mutability: generated, required: true, type: alphanumeric, min_length: 20 }
    storage:
      fields:
        ANON_KEY:
          mutability: generated
          type: derived_jwt
          signing_key: auth.JWT_SECRET     # service.field reference
          claims: { role: anon, iss: supabase }
          alg: HS256
```

Rust side: replace `required`/`optional`/`secret: Vec<String>` with `fields: HashMap<String, FieldPolicy>`, where `FieldPolicy { mutability: Mutability, required: bool, type_spec: Option<FieldType> }` and `Mutability::Generated` carries the `SecretGenerator`-shaped type info (hex/base64/alphanumeric/uuid/derived_jwt) from the original design. Keep a `Deserialize` compatibility shim: a bare string list under a legacy `secret:`/`required:`/`optional:` key still parses, mapped to `{mutability: generated, required: true}` / `{mutability: fixed, required: true}` / `{mutability: fixed, required: false}` respectively, so existing `stacker.yml` files don't break.

### 2. Publish-time validation (no detection, just enforcement)

In `src/routes/marketplace/creator.rs`:
- **Gate: at least one successful deployment.** Add `source_project_id: Option<i32>` to `CreateTemplateRequest`, persisted on `stack_template_version`. New `ensure_has_successful_deployment(pool, project_id)` queries `db::deployment::fetch_by_project` and requires a terminal-success status row (matching existing vocabulary, e.g. `"completed"`/`"running"` per `force_complete.rs`). Missing `source_project_id` fails closed.
- **Gate: contract completeness.** Before accepting a submission, walk the stack's env/service fields already flagged secret-looking by the existing `is_secret_env_key()` heuristic (reused as a *warning* signal only, not a redactor) and require every one of them to have a `mutability: generated` entry in `config_contract.services.*.fields`. Non-secret fields are not required to be declared (undeclared fields default to `fixed`, i.e. today's verbatim behavior) — only the secret-shaped ones are gated, keeping the publish requirement focused on the original security problem while the schema itself supports more. If any secret-shaped field is undeclared, reject with a specific list of unmapped fields — this replaces trusting the blind `confirm_no_secrets` checkbox with an actual, checkable requirement.
- **No auto-redaction step.** Since generation is policy-driven and re-run at install time (not detected/stripped from a submitted blob), the literal values submitted by the author can be treated as their own dev/test values and simply are not the ones installers receive for `generated` fields — see step 4. If we still want defense-in-depth against literal secrets sitting in the stored `stack_definition`, reuse `td-audit`'s existing `check_no_secrets` scan (`crates/td-audit/src/compose/validator.rs`) as a review-time warning (already exists, `admin.rs` security-scan endpoint) — no new detector code needed.

### 3. Persistence + federation

- No new detection-manifest table needed — `config_contract` already travels inside `stack_definition`/`config_files` today. Confirm (implementation-time check) that `create_handler`/`update_handler` actually preserve `config_contract` verbatim through to what's stored in `stack_template_version` and through to `MarketplaceWebhookPayload`/`marketplace_templates` federation — if it's currently dropped anywhere in that chain, that's the one persistence fix required.
- Add `source_project_id INT NULL` to `stack_template_version` (for the deployment gate only).

### 4. Install-time generation

This is the part that must actually run policy-driven generation, since Install Service is Python and the policy now lives in `config_contract` JSON (forwarded as part of `stack_definition`/install payload — already confirmed to flow through today).

In `tools/v1/additional/custom_stack_mapper.py`, before writing the cached compose to disk (~line 666):
- New `_apply_field_policy(compose_text, config_contract, installer_overrides)`:
  - No-op if no `config_contract.fields` entries present (legacy templates keep today's verbatim behavior — no regression).
  - For fields with `mutability: fixed` — leave the author's value untouched (today's behavior).
  - For fields with `mutability: editable` — if the installer supplied an override value in the install payload (a new, small addition to the install request shape — installer's form submission, validated against the field's `type`/`values` constraint), use it; otherwise keep the author's value as the default. This is the `LOG_LEVEL` case.
  - For fields with `mutability: generated` — in dependency order (independent fields first, `derived_jwt` after its `signing_key` dependency is generated): `hex`→`secrets.token_hex(length//2)`, `base64`→`secrets.token_bytes`+urlsafe b64, `alphanumeric`→`secrets.choice` loop respecting `min_length`, `uuid`→`uuid.uuid4()`, `derived_jwt`→`jwt.encode(claims, signing_value, alg)` via PyJWT (add to `tools`/`install` requirements if not already present).
  - Replace each `editable`/`generated` field's value in the compose text with the resolved value.
  - Borrow Ansible's `password`-lookup idempotency pattern: if this same project/install is re-applied (redeploy, not a fresh install), persist `generated` values keyed by `(install_id, field_path)` so a redeploy doesn't silently rotate secrets out from under a running app — check for an existing generated-value store before generating anew. `editable` overrides are simply re-read from the installer's stored install config on redeploy, not regenerated.
- After generation, write each installer's `generated` values into their own project via Stacker's existing remote-secret endpoints (`src/routes/project/secret.rs`, backed by `db::remote_secret::upsert_service_secret`) — this is the concrete tie-in to the existing `stacker secrets` engine, so `stacker secrets list`/`get` immediately shows the installer's own generated values (redacted) via the existing `hydration.rs::redact_app_environment` path. `editable` (non-secret) values are just regular install config, not routed through the secrets engine.

### 5. Local CLI parity

Extend `generate_secrets_script_content()`/`is_secret_env_key()` (`init.rs:1910-1991`) so `stacker init`'s local `generate-secrets.sh` reads `mutability: generated` field policy (type/length/derived) instead of hardcoding `openssl rand -hex 16` for every secret-looking key — keeps local dev generation and marketplace install generation consistent, using one declared policy for both. `editable`/`fixed` fields aren't touched by this script — they're not a generation concern.

### 6. Web UI (Stack Builder) parity

Investigation confirmed `config_contract` today has **zero reach** outside the CLI binary — `hydration.rs` (which is what the backend actually serves for a project) never touches it, no API exposes it, and Stack Builder's frontend is not present in any repo on disk here (`stacker`/`user`/`install`/`tools`) — only its backend consumer (`custom_stack_mapper.py`, receiving flat `{key, value}` pairs with no type/secret metadata) is visible. So "mirror the manifest into Stack Builder" is two separable pieces of work, and only the first is buildable from these repos:

- **(In scope here) Make the persisted, DB-backed copy of `config_contract` the live source of truth**, not the local `stacker.yml` file. Reuse the existing sync pipeline that already moves `env`/`config_files` from CLI push into project storage (`src/project_app/upsert.rs`, which stores raw content to Vault on `stacker deploy`) — add `config_contract` as one more field riding that same pipeline, so a `stacker deploy`/push after editing `stacker.yml` locally persists the policy server-side.
- Expose it on the existing project config API (`src/routes/project/app.rs`, the `GET /project/{project_id}/apps/{code}/config` route already surfaced by `hydration.rs`) — add `config_contract` to `HydratedProjectApp` and accept it on the corresponding update route, so any web client (including Stack Builder, once updated) can read and write it through the same endpoint it already uses for other project config.
- **(Out of scope here) Actual Stack Builder form controls** (type/length/derived-value pickers for a field) require changes in Stack Builder's own frontend repo, which isn't checked out in this environment. This plan delivers the backend contract (schema + API) that frontend would need to consume — building the UI itself needs a follow-up in that repo.
- Practical effect once backend-only work lands: a project created via CLI with `config_contract` set, then pushed/deployed, will have its policy available through the project API immediately — Stack Builder just won't render it as form fields until its frontend adds support. Conversely, nothing here regresses existing Stack Builder behavior for projects with no `config_contract`.

### 7. Rollout for existing templates

- Templates with no `config_contract` field policy: publish gate (step 2) requires `generated` declarations for secret-shaped fields going forward; existing already-approved templates are unaffected until their next version bump, at which point the completeness gate applies. `editable`/`fixed` fields have no gate — they're an additive capability, not a security requirement.
- No auto-unpublish of legacy templates — surface a dashboard warning ("no secret policy declared — buyers may receive the author's original secrets") via the existing `verifications` JSONB column pattern used elsewhere in `admin.rs`.

## Verification

TDD throughout, per the mandate above: every bullet below is written as a failing test *before* the corresponding implementation step in the design section.

- **Contract-conformance tests (write first, both languages)**: a Rust test in `stacker` and a Python test in `tools`/`install` that each load `shared-fixtures/api-contracts/marketplace-field-policy.json` directly and assert their respective type's (de)serialization accepts every example in it (including `fixed`/`editable`/`generated` variants) and rejects a deliberately malformed variant (missing `signing_key` on a `derived_jwt`, unknown `mutability` value). These are the tests that must exist before `TargetConfigContract`'s schema extension or the Python generator are implemented.
- **Rust unit tests** (colocated `#[cfg(test)]`): `TargetConfigContract`/`FieldPolicy` custom deserialize — legacy shorthand lists (`required`/`optional`/`secret`) map to the correct default `mutability`, and full per-field policy form parses correctly; `ensure_has_successful_deployment` gate (400 with no/failed deployments, success with a `"completed"`/`"running"` row); contract-completeness check flags an unmapped secret-looking field and passes when fully declared as `generated`; `config_contract` round-trips through `project_app/upsert.rs` push and appears in `HydratedProjectApp`/the project config GET route.
- **Python tests** (Install Service/tools): `_apply_field_policy` — two installs produce different values for the same `generated` field; a `derived_jwt` field verifies (`jwt.decode`) against its own install's generated signing secret; an `editable` field takes the installer's override when supplied and falls back to the author's default otherwise; redeploy of the same install reuses previously generated values and previously submitted overrides (idempotency); empty/no-policy `config_contract` is a no-op (legacy regression safety).
- **End-to-end**: publish a Supabase-style stack with an incomplete `config_contract` → rejected with the specific missing-field list. Complete the contract, publish without a prior deployment → rejected (deployment gate). Deploy once, resubmit → accepted. Approve, confirm `marketplace_templates` federation retains `config_contract`. Install as two different "buyers" → diff compose files (values differ from each other and from the author's originals); decode each install's generated `ANON_KEY` and verify it validates against that same install's generated `JWT_SECRET`. Redeploy one buyer's install → confirm values are stable, not rotated. Confirm `stacker secrets list` for each buyer's project shows their own values as `[REDACTED]`.
- Run `SQLX_OFFLINE=true cargo test` in `stacker` and `cargo sqlx prepare` after query changes, per this repo's CLAUDE.md rules.

## Critical files

- `~/work/try.direct/config/shared-fixtures/api-contracts/marketplace-field-policy.json` (new canonical contract — write/update first)
- `~/work/try.direct/config/shared-fixtures/README.md` (document the new contract per its own process)
- `src/cli/config_parser.rs` (`TargetConfigContract`/`ConfigContract` schema extension)
- `src/cli/config_contract.rs` (`suggest_contract` — optionally teach it to suggest policy shape, still human-reviewed)
- `src/console/commands/cli/init.rs` (`generate_secrets_script_content`, `is_secret_env_key` — policy-driven local generation)
- `src/project_app/upsert.rs` (persist `config_contract` server-side on CLI push, alongside existing `env`/`config_files` handling)
- `src/project_app/hydration.rs`, `src/routes/project/app.rs` (expose `config_contract` on the project config API for web clients)
- `src/routes/marketplace/creator.rs` (publish gates)
- `src/routes/marketplace/admin.rs` (existing `check_no_secrets` review scan)
- `src/models/marketplace.rs`, `src/db/deployment.rs`
- `src/connectors/user_service/marketplace_webhook.rs` (confirm `config_contract` federates through)
- `tools/v1/additional/custom_stack_mapper.py` (install-time policy-driven generation)
- `user/app/installations/views.py` (confirm `config_contract` forwarded in install payload)
