# Lifecycle of one secret value

Tracing a single value — a database password — from the author's laptop to a
buyer's cloned server. This is the reference for deciding **when a literal must
become a `${VAR}` reference**, and which component is responsible at each point.

Related: [FIELD_POLICY.md](FIELD_POLICY.md) (author-facing guide to declaring the
policy) and [ONE_CLICK_DEPLOY.md](ONE_CLICK_DEPLOY.md) (the clone path).

## The journey

`<pw>` below stands for the author's literal password.

| # | Stage | Where it runs | Form of the value |
|---|-------|---------------|-------------------|
| 1 | Author's `.env` | author's machine | `POSTGRES_PASSWORD=<pw>` |
| 2 | `stacker.yml` parsed, `${...}` expanded | **stacker CLI** (`src/cli/config_parser.rs`, `resolve_env_vars_with_fallback`) | literal |
| 3 | Compose rendered | **stacker CLI** (`src/console/commands/cli/deploy.rs`, `ComposeDefinition::try_from`) | literal |
| 4 | Protected keys turned back into references | **stacker CLI** (`parameterize_compose_env_vars`) | `${POSTGRES_PASSWORD}` |
| 5 | Config bundle posted to the backend | stacker CLI → **stacker server** (`src/routes/project/deploy.rs`) | compose with references, `.env` with literals |
| 6 | Queued, then Terraform + Ansible | **install service** (`src/connectors/install_service/client.rs`) | unchanged |
| 7 | Files land in `/home/trydirect/<stack>/` | **target machine** (build box) | compose + co-located `.env` |
| 8 | `docker compose up` | **target machine** | literal, resolved from `./.env` — **and Postgres writes it into its data directory** |
| 9 | Finalize before the snapshot | `bake` binary, over SSH to the **target machine** (`src/helpers/bake_finalize.rs`) | stack stopped and data volumes dropped, embedded secrets replaced, references outside the contract put back to their values, `.env` cleared, machine identity and the author's own access stripped |
| 10 | Snapshot taken | Hetzner API → **baked image** | only `${POSTGRES_PASSWORD}`; no value anywhere |
| 11 | Cloud-init rendered for a clone | **stacker server** (`src/routes/oneclick_deploy/clone.rs`) | a fresh value per buyer |
| 12 | First boot: `/etc/stacker/env` copied over `./.env` | **buyer's machine** | new literal |
| 13 | `docker compose up` | **buyer's machine** | Postgres initializes from scratch |

Stage 8 is the one that surprises people: the value does not only sit in files,
it is written **into the data directory**. Replacing it in compose is therefore
not enough — the volume has to be dropped at stage 9, or the buyer's server
keeps authenticating with the author's password.

## When a literal needs a reference

A value must become `${VAR}` when **both** hold:

1. it survives the snapshot (it is in a file, or in a volume that is kept), and
2. it must differ per buyer.

If only the first holds, leave it literal (`OLLAMA_MODEL: llama3.1`). If only the
second holds, there is nothing to replace.

The mirror rule matters just as much: **every `${VAR}` baked into the image must
have something that fills it on the buyer's machine.** There are exactly two
such sources.

| Class | Form in the baked compose | Filled on the buyer's machine by | Produced at |
|-------|---------------------------|----------------------------------|-------------|
| contract `generated` | `${VAR}` | cloud-init regeneration commands | stage 11, stacker server |
| contract `provided` | `${VAR}` | the buyer's submitted values | stage 11, stacker server |
| contract `fixed` | literal | nothing needed | — |
| not declared in the contract | literal | nothing needed | — |
| `${X:-default}` | left as written | its own default | stage 13, buyer's machine |
| `$$`-escaped | left as written | never interpolated at all | — |

`config_contract` is the only authority on what is sensitive. Nothing is guessed
from variable names: name heuristics both miss real secrets and clear harmless
values, and the author already stated which fields matter.

### Variables the author parameterized outside the contract

An author may write `${SOMETHING}` in `stacker.yml` for a value that is not a
contract field. That reference is expanded at stage 2, but stage 4 turns plain
top-level `env:` keys back into references — and nothing fills those on the
buyer's machine. **Stage 9 therefore puts their value back**, so the image holds
a literal: the same for every buyer, which is exactly what a non-contract value
is. `${X:-default}` and `$$`-escaped text are left alone, since neither needs a
source.

A reference with no value on the build box and no default fails the bake rather
than shipping an image that boots with an empty string.

The single exception is a literal that *contains* a contract secret, such as
`DATABASE_URL=postgresql://user:<pw>@db:5432/app`. Stage 9 rewrites the embedded
part to `${POSTGRES_PASSWORD}`, because the surrounding key name (`DATABASE_URL`)
is not something any policy mentions, and a name-based rule cannot see it.

Consequence worth accepting deliberately: a secret the author never declared
stays in the image. That follows directly from making the contract the only
authority.

## What else must not survive the snapshot

A secret value is not the only thing a snapshot carries forward. Two more are
removed at stage 9, both for the same reason — they are specific to the author's
machine, and a snapshot turns "specific to one machine" into "shared by every
buyer":

| What | Why it matters |
|------|----------------|
| SSH host keys, `machine-id`, cloud-init instance state | every clone would present the same host identity, so one buyer can impersonate another buyer's server |
| `authorized_keys`, private keys, `known_hosts`, `~/.docker/config.json` | cloud-init *appends* the buyer's key rather than replacing the file, so an author key left in the image grants its holder root on every server cloned from it |

`sshd` regenerates host keys on first boot when none are present, and `systemd`
repopulates an empty `machine-id`, so removal is enough — nothing has to be
recreated.

Note the consequence for the operator: after stage 9 the build box no longer
accepts the bake key, so a failed bake cannot be retried by reconnecting to the
same machine. Start from a fresh build box instead.

### Order of the finalize steps

The tear-down runs **before** the files are rewritten. `docker compose down`
parses the compose, and a cleared `.env` would make any `${VAR}` outside an
environment block — `image: ${REGISTRY}/app:${TAG}`, `ports: ["${PORT}:80"]` —
resolve to empty and fail the tear-down, with the files already modified and
nothing to roll back to. Machine identity goes last, because after it the box no
longer accepts the bake key.

### When a finalize fails part-way

The steps are not transactional and most are not reversible, so a failure
reports what already completed. Re-running against the same box is rarely
equivalent: the compose is already sanitized, so the embedded-secret scan has
nothing left to find; the `.env` is already cleared, so there are no values to
search for; the data volumes are gone; and after the identity reset the box no
longer accepts the bake key at all. In every one of those cases the answer is a
fresh build box, and the error says so rather than leaving it to be discovered.

### Values a clone would lose

A service declaring `env_file:` reads its values out of that file, not through
`${VAR}`. Stage 12 replaces the file wholesale, so anything in it that is not a
contract field disappears on the buyer's machine — the container then starts
without values it had on the build box. The bake stops and names them; the fix
is to move those values into an `environment:` block, where they become part of
the image.

Stacker's own generator writes values into `environment:`, so this only arises
with a hand-written `deploy.compose_file`. Both spellings of that block are
covered:

```yaml
environment:            environment:
  KEY: value              - KEY=value
```

## When the bake refuses

Sanitizing depends entirely on the contract resolving. If it does not — the
template is not approved yet, `DATABASE_URL` is unset, the query failed, or the
author declared no fields — then nothing is substituted, nothing is cleared, and
an image carrying the author's values would be published with a confident
"Sanitized" line. The bake therefore stops unless `--allow-unsanitized-snapshot`
says the stack genuinely has no secrets.

A project that ships no `.env` is normal and does **not** stop the bake: a
reference the buyer's machine cannot fill is already caught on its own. What the
bake does say out loud is that the embedded-secret scan had no values to search
for, since a mistyped `--project-dir` looks identical from here.

## A reference with no source

When a `${VAR}` is baked in and nothing fills it, three different stages are
involved:

| | Stage | What happens |
|---|-------|--------------|
| **Created** | 4 — stacker CLI | the value is turned into a reference that nothing will fill later |
| **Caught** | 11 — stacker server | the clone compares the image's references against what will actually arrive, and refuses before creating a server |
| **Would surface** | 13 — buyer's machine | Compose substitutes an empty string and only warns; the systemd unit still reports active while the stack is broken |

The refusal at stage 11 is therefore not noise — it covers the silent failure at
stage 13. The fix belongs at stage 4: never create a reference that has no source.

Both sets are derived from the same place — the contract
(`required_env_keys` in `src/cli/generator/compose.rs`). Deriving the required
list from the *text* of the compose file instead pulls in things that need no
source at all (`${X:-default}`, `$$`-escaped text) and misses the ones that do.
