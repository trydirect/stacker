# Immutable deploy — build once, clone many

## Why

Every production failure we hit deploying stacks was a **deploy-time dynamic
step**: pulling images (`trydirect/redis` 404), running migrations (missing
grant → 403), building/copying (`built ≠ deployed`), compose synthesis
(`docker-ce` leak, dropped `services[]`). Deploy-time is where reliability dies.

**The reframe:** split the current single "deploy" into two very different phases.

- **BUILD / BAKE** (internal, once per stack version): the *current* deploy
  machinery — provision, pull images, synthesize compose, run migrations, health-
  check — runs here, on a machine we control, gated by a health check. Its output
  is a **validated server snapshot**.
- **DEPLOY** (per user): **clone the baked snapshot + inject per-user env at first
  boot.** Nothing that can assemble-fail happens here.

> Deploy-time is where reliability goes to die. Move all fragile work to
> build-time and validate it there; user deploy becomes near-deterministic.

## Two artifacts, always separate

- **Immutable app snapshot** — OS + docker + all images pulled (digest-pinned) +
  baked compose + DB **schema** + validated healthy. Cloned per user.
- **Persistent data volume** — user DB **contents**. Fresh at first boot, survives
  re-bakes. (The snapshot never contains user data.)

---

## BUILD / BAKE (once per stack version — internal pipeline, not a user request)

This is the *current* deploy path, repurposed as a build step:

1. Provision a Hetzner build box (`cpx21`, `docker-ce`).
2. **Pull every image, digest-pinned** → **fail the bake if any 404.** *(This is
   where `trydirect/apache` dies loudly — once, in front of us, never in front of
   a user.)*
3. Write the **canonical pre-synthesized `docker-compose.yml`** (validated once —
   no synthesis at deploy).
4. Initialize DB **schema only** (migrations run here) — snapshot's DB has
   structure, **zero user data**.
5. `docker compose up` → **health-check every service** (reuse the `td-audit`
   readiness/exposure engines + real probes).
6. Stop cleanly; DB data lives on the volume mount, not root disk.
7. **Snapshot the box** → `HetznerCloudConnector::create_server_snapshot` returns
   `HetznerSnapshot.image_id`.
8. **Only a green bake publishes** the snapshot. Record
   `{stack, version, provider, image_id, image_digests, health: green}`.

## DEPLOY (per user — near-deterministic, seconds)

1. **Clone**: `HetznerCloudConnector::create_server_from_image(token,
   HetznerCreateServerRequest { image_id, server_type, location, ssh_key_ids,
   user_data })` → returns `HetznerProvisionedServer { id, public_ipv4 }`.
   *(In Hetzner a snapshot IS an image, passed as the `image` field.)*
2. Attach a **fresh persistent volume** for DB data.
3. **cloud-init `user_data`** is the *only* per-user variance:
   - **env** → `/etc/stacker/env`: `SECRET_KEY`, DB passwords + admin creds (from
     **Vault**), `DOMAIN`, admin email.
   - **render a handful of files** from env: nginx vhost for the domain, TLS
     request. Tiny + deterministic — the opposite of today's synthesis.
4. A systemd unit runs `docker compose up -d` with the **baked** compose + injected
   env; Let's Encrypt on the domain at boot.
5. Health-check → mark **succeeded**. **No pulls, no migrations, no synthesis at
   deploy.**

---

## Per-buyer field regeneration (field policy)

A baked snapshot freezes whatever value every field held when the source box was
baked. For `mutability: generated` fields that is exactly wrong — the policy means
"a fresh value per install", but every clone of one image would otherwise inherit
the single baked secret (shared JWT/DB creds across all buyers). The clone path
closes that on the box, at first boot, reusing the *same* generators a normal
install's `generate-secrets.sh` runs (one source of truth).

**How it flows:**

1. **Bake pins the policy to the image.** `bin/bake.rs` resolves the template's
   `config_contract` by slug (`get_approved_by_slug` → `get_config_contract`) and
   stores it on `baked_snapshots.config_contract`. The snapshot now carries the
   field policy it was baked with, immutably.
2. **Clone resolves what to regenerate.** `routes/oneclick_deploy/clone.rs` reads
   `snapshot.config_contract` and builds two lists (skipping any field the buyer
   already supplied in `form.env`):
   - `regen_commands` → `(env_key, shell_generator_expr)` for plain `generated`
     fields, via `console::…::init::generator_shell_expression` (the shared
     generator: `openssl rand -hex/-base64`, `tr -dc`, `uuidgen`).
   - `derived_jwt_commands` → `DerivedJwtSpec` for `type: derived_jwt` fields.
3. **The box mints the values.** `helpers/cloud_init.rs::regen_runcmds` emits, into
   the cloud-init `runcmd` list, one command per field that rewrites (or appends)
   `KEY=` in `/etc/stacker/env`, **before** the `stacker-compose.service` restart.

**Per-mutability behavior on clone:**

| Policy | On the cloned box |
|---|---|
| `fixed` | untouched — the baked value is a constant by design |
| `editable` | the buyer's `form.env` override, else the baked default |
| `generated` | a fresh value minted on the box; the baked secret is discarded |
| `generated` + `derived_jwt` | signed on the box (see below) |

**`derived_jwt` signing (HMAC).** The header (`{"alg":..,"typ":"JWT"}`) and claims
are known at build time, so `clone.rs` precomputes their base64url in Rust — no
author data touches the shell. The box does only the part that depends on runtime:
it HMACs `header.payload` with the freshly-written value of the signing field
(`signing_key: "service.FIELD"` → the `FIELD` env var) via
`openssl dgst -sha256|-sha384|-sha512`. These commands are ordered **after** the
plain `generated` fields, so the signing key already exists in `/etc/stacker/env`.
Only HMAC algorithms are supported (the shared secret is on the box); asymmetric
algs are skipped.

**Security properties:** two buyers of the same snapshot never share a `generated`
value; the author's baked secret is discarded, not shipped.

**Limits:** this only fixes values the app reads from env each boot. Anything the
app persisted on the source box's first run (into its DB/volume) is frozen in the
snapshot — that needs a post-clone rotation or a "clean" bake taken before
first-run materialization. Regenerated values are minted on the box only and are
not (yet) written back into the `stacker secrets` engine.

This is the immutable-path counterpart to the normal marketplace install path,
where the Python Install Service applies the same policy — see
`docs/MARKETPLACE_FIELD_POLICY.md` (§9 for this path, §8 for the federation that
feeds the normal path).

## Failure semantics (the whole point)

- **Bake** can fail on: missing image, unhealthy service, bad compose → caught
  once, by us; the snapshot is not published.
- **Deploy** can essentially only fail on: Hetzner API / network — not app
  assembly. "First-deploy-succeeds" approaches the provider's uptime.

## Updates & the one honest remaining migration

- New version = **new bake → new snapshot → blue-green**: new server from the new
  snapshot, re-attach the data volume, cut over.
- **Schema upgrades across versions** against the persistent volume are the one
  place migrations survive — but as an explicit, controlled upgrade job, not a
  per-deploy step.

## Maps onto what exists

| Piece | Where |
|---|---|
| Snapshot create (`image_id`) | `src/connectors/hetzner.rs::create_server_snapshot` ✅ |
| **Clone from snapshot** (new) | `src/connectors/hetzner.rs::create_server_from_image` ✅ (this branch) |
| Bake health gate | `td-audit` readiness/exposure engines + probes |
| Secrets at boot | Vault (`helpers/vault.rs`) |
| Baked compose | the synthesized artifact, frozen at bake |
| Field policy pinned to image | `baked_snapshots.config_contract`, set by `bin/bake.rs` |
| Per-buyer field regeneration | `routes/oneclick_deploy/clone.rs` (`regen_commands`, `derived_jwt_commands`) → `helpers/cloud_init.rs::regen_runcmds` |
| "Does it deploy" harness | *is* the bake health-gate |

## Per-provider

`image_id` is Hetzner-only. DO/AWS need their own snapshot/AMI ids → a **bake
matrix** (stack × provider × region). Start Hetzner-only; expand later.

---

## Smallest proof (do this first, nothing else)

Bake **LAMP once**, then **clone it 5×** with 5 different domains/envs via
`create_server_from_image` + cloud-init. **5/5 boot healthy** validates the model
*and* fixes the exact thing that failed us. Then generalize.

### Buildable slices, in order
1. `create_server_from_image` on the Hetzner connector — **done in this branch** (stub + request/response types; needs a live-API integration test).
2. A **bake job** (repurpose the deploy path): provision → pull(pinned) → compose up → health-gate → snapshot → register `image_id`.
3. A **cloud-init template**: env + secrets(Vault) + the small file render + systemd `compose up`.
4. The **clone deploy path**: `create_server_from_image` + attach volume + cloud-init, replacing the OpenTofu/copy path for baked stacks.
5. Snapshot **registry** (stack/version/provider → image_id, health, digests) + prune.

## Affected repos

- **stacker** (this branch `feature/immutable-deploy`): connector clone method,
  bake job, deploy path swap, snapshot registry.
- **install service** (Python): the OpenTofu/copy provisioning it does today is
  what the bake step *becomes* / the clone path *replaces* for baked stacks — a
  Phase-2 branch when we wire the bake job and clone deploy end-to-end.

## Scope honesty

This is a real re-architecture of the deploy path (build/bake pipeline + volume
split + cloud-init render), not a patch. The 5×-clone proof is a few days and
de-risks the whole bet before committing.
