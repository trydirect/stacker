# Marketplace per-install billing — payment flow

## What this is

A billing model for marketplace templates where the buyer is charged **on
every successful install**, not once for perpetual access. Vendors opt in
per template by setting `billing_cycle: "per_install"` alongside a positive
`price`. Coexists with the existing `one_time` model — legacy purchases
(e.g. umami's $10 one-time) are unaffected.

The whole design rides on a single property:

> **The buyer's card is only charged if the install actually produces a
> running stack.** No successful deploy → no money moves.

That constraint is what forces the two-phase dance below.

## Actors

| Actor | Responsibility |
|---|---|
| Buyer's CLI (`stacker install`) | Generates the idempotency key. Sends the install request. Retries safely with the same key. |
| Stacker server | Coordinator. Runs the access gate, tells user_service when to authorize / capture / void, maintains a local `marketplace_install_authorization` ledger for reconciliation. |
| user_service | Source of truth for money movement. Owns the Stripe integration, the payment intent, refund history, and idempotency semantics. |
| Deploy agent | Runs the deploy on the target server. Signals success via `POST /deploy-complete`. |

**Important:** stacker never touches Stripe directly for this feature. It
only calls user_service HTTP endpoints. If user_service is down, no
per-install charges can move — but existing `one_time` installs (which
consult the ownership flag) still work.

## Core idea — hold, then take

Think of a hotel check-in. They don't charge you the $200 when you walk
in — they place a $200 **hold** on your card, then convert the hold to a
real charge when you actually check out. If you never check in, the hold
expires and you're never charged.

| Term | Meaning here |
|---|---|
| **Authorize** | "Hold $9.99 on the buyer's card." Money is reserved, not moved. Returns an opaque `authorization_id`. |
| **Capture** | "Convert the hold into a real charge." Money moves from buyer to vendor's Stripe Connect account. |
| **Void** | "Release the hold." Nothing was charged. |

The whole per-install flow is: authorize before install starts, capture
when deploy confirms success, void otherwise.

## Happy path

`stacker install paid-per-install` for a template priced $9.99:

```
Buyer                Stacker CLI              Stacker server           user_service           Deploy agent
  |                     |                          |                       |                       |
  |  stacker install    |                          |                       |                       |
  |-------------------->|                          |                       |                       |
  |                     |  POST /install           |                       |                       |
  |                     |  Idempotency-Key: cli-x  |                       |                       |
  |                     |------------------------->|                       |                       |
  |                     |                          |                       |                       |
  |                     |    (1) access gate: is user allowed?             |                       |
  |                     |                          |--can_charge?--------->|                       |
  |                     |                          |<--yes-----------------|                       |
  |                     |                          |                       |                       |
  |                     |    (2) authorize $9.99                           |                       |
  |                     |                          |--authorize(idem-x)--->|                       |
  |                     |                          |                       | (Stripe: hold $9.99   |
  |                     |                          |                       |  on buyer's card)     |
  |                     |                          |<--auth_id=A123--------|                       |
  |                     |                          |                       |                       |
  |                     |    (3) insert auth row (status=authorized)                               |
  |                     |    (4) insert project row                                                |
  |                     |    (5) attach project_id to auth row                                     |
  |                     |    (6) queue deploy → deployment_hash D999                               |
  |                     |    (7) attach D999 to auth row                                           |
  |                     |                          |                                               |
  |                     |<--200 { auth: A123, ...} |                                               |
  |<--"install started" |                          |                                               |
  |                     |                          |                                               |
  |                     |                          |          (agent runs the deploy)              |
  |                     |                          |                                               |
  |                     |                          |          POST /deploy-complete                |
  |                     |                          |          { deployment_hash: D999 }            |
  |                     |                          |<------------------------------------------|
  |                     |                          |                                               |
  |                     |    (8) look up auth by D999 → A123 (authorized)                          |
  |                     |    (9) capture A123                                                       |
  |                     |                          |--capture(A123, D999)->|                       |
  |                     |                          |                       | (Stripe: convert hold |
  |                     |                          |                       |  to real $9.99 charge |
  |                     |                          |                       |  → vendor's account)  |
  |                     |                          |<--captured------------|                       |
  |                     |    (10) mark auth row status=captured                                    |
```

**Money actually moves at step (9).** Everything before is a hold, not a
charge.

## Failure paths

| What breaks | When | Money outcome |
|---|---|---|
| Buyer has no card on file | Step (1), `can_charge=false` | HTTP 402 to CLI. Nothing held. No project row. |
| Card declined | Step (2), authorize returns 402 | HTTP 402 to CLI. Nothing held. No project row. |
| Stacker server crashes between (2) and (5) | Process killed after authorize | The **sweeper** finds the row past `expires_at + 5min` grace, voids the auth via user_service, marks the row `voided`. Hold released. |
| `build_project_form` or `insert_project_from_form` fails | Steps (3)–(5) | Guard spawns a fire-and-forget void with `reason="install_failed:<stage>"`. Row → `voided`. Buyer's card is never charged. |
| Deploy agent never confirms | `deploy-complete` never arrives | Same as server crash — sweeper voids after TTL. Buyer's card is never charged; the hold falls off at Stripe. |
| Buyer retries with same `Idempotency-Key` | Duplicate `POST /install` | user_service returns the *same* auth handle. Stacker's `(user_id, idempotency_key)` unique index returns the existing row. **One authorization, no double-hold.** |
| Buyer retries with a **different** key | Different key | Two separate holds. This is why the CLI reuses the key across retries. |

## Idempotency

Every per-install request carries an `Idempotency-Key`:

- **CLI:** generates `cli-<uuid>` once per invocation and threads it into
  the request. Honors `STACKER_INSTALL_IDEMPOTENCY_KEY` env var so CI /
  scripted retries can pin it.
- **Server:** if a request arrives without a key on a per_install
  template, the server generates `srv-<uuid>`, logs a warning, and echoes
  the effective key in the response so a retrying client can pin it.
- **DB:** `(user_id, idempotency_key)` is the unique index on
  `marketplace_install_authorization`. Same key → same row.
- **user_service:** authoritative on idempotency of the underlying
  payment intent. Same key with same body → same handle. Same key with
  different body → HTTP 409.

The response echoes the effective key so a retrying client can reuse it.

## Sync install vs. async deploy

Two capture sites, chosen by whether the install request carried a
`deploy` block:

| Scenario | `request.deploy` | Capture site |
|---|---|---|
| **Normal flow** — buyer wants the stack deployed | `Some(...)` | `deploy_complete_handler` looks up auth by `deployment_hash` and captures. |
| **Install-only** — buyer wants only the project row created (rare) | `None` | `install_stack_template` synthesizes `deployment_hash = "install-only:<project_id>"` and captures inline before returning. |

The install-only path exists because the value paid for is the install
artifact itself — the project row + written `stacker.yml`. If the buyer
never intended to deploy, they still get what they paid for.

## The local ledger

The `marketplace_install_authorization` table (migration
`20260718130000`) is stacker's local view. Every row goes through this
state machine:

```
                ┌──────────────┐
    insert  ──▶ │  authorized  │──── capture ──▶ ┌──────────┐
                │              │──── void ─────▶ │ voided   │
                │              │──── expire ───▶ │ expired  │(sweeper)
                └──────────────┘                 └──────────┘
                                                 ┌──────────┐
                                                 │ captured │
                                                 └──────────┘
```

Terminal states (`captured`, `voided`, `expired`) are immutable.

**Why keep a local ledger when user_service is authoritative?** Two
reasons:
1. Sweeper needs a queryable list of "authorized past their TTL" without
   asking user_service to page through every payment intent.
2. Reconciliation. Every 60s the sweeper detects drift — e.g. user_service
   already captured but our row still says `authorized` (409 from void →
   flip local row to `captured`).

## Sweeper

`src/services/install_authorization_sweeper.rs`. Ticks every 60s
(configurable constant). Each tick:

1. Query rows with `status='authorized' AND expires_at < now() - 5min`.
2. For each: call `void_install_charge(service_token, auth_id, "expired")`.
3. On success → `mark_voided`.
4. On 409 (user_service says already captured) → `mark_captured`
   (drift reconciliation).
5. On transport failure → warn, retry next tick.

**The sweeper is a cleanup tool, not the source of truth.** Even if it's
down, user_service's own `expires_at` on the payment intent auto-voids
independently — the hold falls off the buyer's card either way. What the
sweeper adds is prompt local-DB reconciliation.

Sweeper is spawned from `src/startup.rs` and no-ops when the feature flag
is off.

## Feature flag

`STACKER_PER_INSTALL_BILLING_ENABLED` env → `Settings::per_install_billing_enabled`. Default `false`.

When off:
- `install_stack_template` treats `billing_cycle="per_install"` as
  `one_time` (no authorize, no capture).
- The access gate's `is_per_install_effective` returns false; the
  `can_charge` probe is skipped.
- The sweeper doesn't spawn.

Result: **all existing behavior is preserved bit-for-bit when the flag is
off**, no matter what `billing_cycle` values sit in the DB.

Per-template opt-in is via `billing_cycle="per_install"` set at submit or
update time. Rollout:

0. Ship code + tests, flag off. Verify no behavior change for umami.
1. Enable in staging with an internal test template.
2. Enable in prod, allow-list one vendor, watch
   `count(voided|expired) / count(*)` ratio.
3. GA.

## Code map

| Concern | File |
|---|---|
| DB schema | `migrations/20260718130000_marketplace_install_authorization.up.sql` |
| Ledger CRUD | `src/db/marketplace_billing.rs` |
| Connector trait | `src/connectors/user_service/connector.rs` |
| Real HTTP client (`/api/1.0/marketplace/billing/*`) | `src/connectors/user_service/client.rs` |
| Deterministic mock (default-success) | `src/connectors/user_service/mock.rs` |
| Scriptable test double | `TestUserService` in `src/services/marketplace_access.rs::tests` |
| Types (handles, capabilities) | `src/connectors/user_service/types.rs` |
| Access gate (`can_charge` probe) | `src/services/marketplace_access.rs` |
| Install path (authorize + ledger + capture) | `src/routes/marketplace/install.rs::install_stack_template` |
| Deploy-complete capture | `src/routes/marketplace/public.rs::deploy_complete_handler` |
| Sweeper | `src/services/install_authorization_sweeper.rs` |
| Sweeper spawn | `src/startup.rs` |
| Settings flag | `src/configuration.rs::Settings::per_install_billing_enabled` |
| CLI: idempotency key + response echo | `src/console/commands/cli/marketplace.rs` + `src/cli/stacker_client.rs` |
| CLI: `/install` price suffix | `src/console/commands/cli/marketplace.rs::display_plan` |

## HTTP surface stacker calls on user_service

All under `/api/1.0/marketplace/billing/`. Auth: user's bearer token
(install path) or the shared `STACKER_SERVICE_TOKEN` (deploy-complete,
sweeper).

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/can-charge` | — | `{ can_charge: bool, reason: Option<String> }` |
| POST | `/authorize` | `{ template_id, amount_minor, currency, idempotency_key }` | `AuthorizationHandle` — also echoed via `Idempotency-Key` header |
| POST | `/capture` | `{ authorization_id, deployment_hash }` | Updated `AuthorizationHandle` |
| POST | `/void` | `{ authorization_id, reason }` | 204 |

Error mapping in `src/connectors/user_service/client.rs::map_billing_error_status`:

- 402 → `ConnectorError::PaymentRequired` → HTTP 402 to the CLI
- 409 → `ConnectorError::Conflict`
- 401/403 → `Unauthorized`
- 5xx → `ServiceUnavailable`

## The one thing to internalize

**Stacker doesn't hold money.** It's a coordinator that tells user_service
when to authorize, capture, and void. The `marketplace_install_authorization`
table is a local mirror so stacker can reconcile if state drifts. If you
find yourself reasoning about "how much did the buyer pay" — that
question is answered in user_service, not in this repo.

## Related documents

- Implementation plan: `.claude/plans/per-install-charge-pay-ethereal-cray.md`
- BDD scenarios (pending step defs):
  `tests/features/marketplace_per_install_billing.feature.pending`
- Legacy `one_time` billing behavior: still lives in
  `validate_marketplace_template_access` — the `is_per_install` early
  return runs *before* the ownership check that gates `one_time` templates.
