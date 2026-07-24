# Stacker Test Plan

## Coverage summary

Integration tests: 94 files, 468+ tests — good breadth on happy paths.
Unit tests: sparse — most modules have no inline `#[cfg(test)]` blocks.

---

## Priority 1 — Auth middleware (9 files, 0% unit coverage)

**Risk:** Token forgery, expiry bypass, HMAC replay are blind spots.

| File | What to test |
|---|---|
| `f_jwt.rs` | No header → skip; non-Bearer → skip; malformed JWT → skip; expired JWT → **Err** (not skip); valid JWT → Ok + extensions set; double-auth → Err |
| `f_hmac.rs` | No stacker-id → skip; has id, no hash → Err; wrong HMAC → Err; correct HMAC → Ok |
| `f_cookie.rs` | No cookie → skip; no access_token cookie → skip; correct extraction from multi-cookie string |
| `f_query.rs` | Non-MCP path → skip; no query string → skip; no access_token param → skip; URL-encoded token decoded correctly |
| `f_agent.rs` | No x-agent-id → skip; invalid UUID → Err; no Authorization → Err; non-Bearer → Err |
| `authorization.rs` | `fetch_policy_fingerprint` returns consistent (max_id, count) pair |

**Status:** ✅ Done — 17 tests, all passing. See `#[cfg(test)]` in each file.

---

## Priority 2 — Payout webhook / Stripe signature

**Risk:** Stripe signature bypass → financial impact.

**Note:** signature logic lives in `services/payout_provider.rs`
(`verify_stripe_signature`); the `payout_webhook.rs` handler is thin delegation.
Tests added at the provider level (pure, no HTTP). 7 pre-existing tests; the
security-critical bypass cases were missing and are now added.

| Scenario | Expected | Covered by |
|---|---|---|
| Missing `Stripe-Signature` header | Err "Missing Stripe-Signature" | ✅ `stripe_webhook_rejects_missing_signature_header` |
| **Forged signature** (signed w/ wrong secret) | Err "Invalid signature" | ✅ `stripe_webhook_rejects_forged_signature` |
| **Tampered payload** (valid sig for different body) | Err "Invalid signature" | ✅ `stripe_webhook_rejects_tampered_payload` |
| Stale timestamp (> 300s) | Err "outside tolerance" | `stripe_webhook_rejects_stale_signature` (pre-existing) |
| Missing `t=` part | Err "missing timestamp" | ✅ `stripe_webhook_rejects_missing_timestamp_part` |
| Missing `v1=` part | Err "missing v1 signature" | ✅ `stripe_webhook_rejects_missing_v1_part` |
| Non-hex `v1` value | Err "hex is invalid" | ✅ `stripe_webhook_rejects_invalid_hex_signature` |
| Valid sig, non-`account.updated` event | Ok(None) → "Webhook ignored" | ✅ `stripe_webhook_ignores_non_account_updated_event` |
| Valid sig, `account.updated` missing data.object | InvalidResponse | ✅ `stripe_webhook_account_updated_missing_object_is_invalid` |
| Valid sig + timestamp → parsed | Ok(Some(update)) | `stripe_webhook_parses_valid_account_update` (pre-existing) |

**Status:** ✅ Done — 15 tests total (8 new), all passing.

---

## Priority 3 — `services/marketplace_access.rs` (access gate for all installs)

**Risk:** Access gate for all marketplace installs.

**Note:** module already had 6 tests (coverage report was inaccurate). Added 5 tests
for the previously-untested error variants and ownership-resolution fallbacks.

| Scenario | Expected | Covered by |
|---|---|---|
| User below minimum plan → denied | `InsufficientFeaturePlan` | `rejects_users_below_marketplace_install_plan` (pre-existing) |
| User meets minimum plan, template requires higher → denied | `InsufficientTemplatePlan` | ✅ `rejects_when_template_requires_higher_plan_than_feature_plan` |
| Missing user token → denied | `MissingUserToken` | ✅ `rejects_when_user_token_missing` |
| Upstream connector error → propagated | `ValidationFailed` | ✅ `propagates_connector_error_as_validation_failed` |
| User owns template (by product_id) → allowed | Ok | `validates_feature_plan_template_plan_and_ownership` (pre-existing) |
| User owns template (by UUID) → allowed | Ok | ✅ `allows_when_user_owns_template_by_uuid` |
| User owns template (by slug) → allowed | Ok | ✅ `allows_when_user_owns_template_by_slug` |
| Template free / zero-price → always allowed | Ok | `allows_free_templates_without_ownership`, `allows_zero_price_templates_without_ownership` (pre-existing) |

**Status:** ✅ Done — 11 tests total (5 new), all passing.

---

## Priority 4 — DB layer edge cases (20 files, 0% unit coverage)

**Risk:** Concurrent writes, not-found vs. error, missing rows reach callers silently.

**Audit (verified):** all 20 `src/db/*.rs` files have zero inline tests — report was
accurate here. BUT every function takes `&PgPool` and runs real SQL, so these need a
live Postgres (via `#[sqlx::test]` fixtures or the `spawn_app` harness), not inline
unit tests. sqlx `query!` macros already type-check most queries at compile time.
**Deferred** — belongs in a dedicated DB-fixture effort, lower value-per-cost than P5/P6.

Key targets when tackled:
- `db/deployment.rs` — state machine transitions (concurrent update race)
- `db/project.rs` — upsert idempotency
- `db/marketplace.rs` — `get_by_slug_with_latest` returns `SlugLookupError` — verify caller handles both variants

**Status:** Deferred (needs DB harness).

---

## Priority 5 — `services/dag_executor.rs` (0% coverage)

**Risk:** Complex execution path; failures are silent or produce wrong state.

**Audit (verified):** report was accurate — 0 tests. The two pure graph functions
(`topological_sort`, `validate_dag`) are DB-free and hold the complex logic (Kahn's
algorithm, cycle detection). `execute_dag` itself needs a `&PgPool` — deferred to the
DB-harness effort.

| Scenario | Expected | Covered by |
|---|---|---|
| Empty DAG | Err "at least one step" | ✅ `topological_sort_rejects_empty_dag`, `validate_dag_rejects_empty` |
| Linear chain a→b→c | 3 ordered levels | ✅ `topological_sort_orders_linear_chain` |
| Parallel a→b, a→c | b,c share one level | ✅ `topological_sort_groups_parallel_steps_in_same_level` |
| Cycle a→b→a | Err "cycle" | ✅ `topological_sort_detects_cycle` |
| Self-loop a→a | Err "cycle" | ✅ `topological_sort_detects_self_loop` |
| Edge to unknown step | ignored, not treated as dep | ✅ `topological_sort_ignores_edges_to_unknown_steps` |
| Disconnected roots | share first level | ✅ `topological_sort_handles_disconnected_nodes_in_first_level` |
| Missing source / target | Err "source/target step" | ✅ `validate_dag_requires_a_source`, `_a_target` |
| Valid source+target (incl. ws_/grpc_) | Ok | ✅ `validate_dag_accepts_source_and_target`, `_alternate_...` |

**Status:** ✅ Done (graph logic) — 12 tests, all passing. `execute_dag` DB path deferred.

---

## Priority 6 — Form validation gaps

**Risk:** Invalid data reaches DB silently.

**Audit (verified):** 28 forms lack tests, but most are trivial data structs whose only
"validation" is serde_valid `min_length`/`max_length` attrs — testing those tests the
*library*, not our code. The **one target with real custom logic** is
`deploy.rs::validate_cloud_instance_config` (pure; guards every cloud deploy).
`app.rs` (176 loc) is standard serde_valid attrs — low value.

| Scenario | Expected | Covered by |
|---|---|---|
| provider "own" → skip instance checks | Ok even with empty fields | ✅ `own_provider_skips_instance_validation` |
| Cloud provider, all fields present | Ok | ✅ `cloud_provider_with_all_instance_fields_passes` |
| Cloud provider, all fields missing | Err listing region+server+os | ✅ `cloud_provider_missing_all_instance_fields_is_rejected` |
| Cloud provider, one field missing | Err listing only that field | ✅ `cloud_provider_missing_single_field_is_rejected` |
| Empty string treated as missing | Err | ✅ `empty_string_instance_field_counts_as_missing` |

**Status:** ✅ Done (the one real target) — 5 tests, all passing. Remaining forms are
trivial serde_valid attrs, not worth unit tests.

---

## Modules with full coverage (do not regress)

- `cli/` — 30+ files, comprehensive unit tests
- `helpers/redact.rs`, `helpers/security_validator.rs`, `helpers/ip.rs`
- `forms/cloud.rs`, `forms/port.rs`, `forms/var.rs`, `forms/volume.rs`
- `connectors/admin_service/jwt.rs`
- `middleware/authentication/method/f_oauth.rs`
- Security tests: `tests/security_*.rs` (12 files)
