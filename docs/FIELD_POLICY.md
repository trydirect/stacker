# Field policy — per-buyer secrets for marketplace templates

This guide is for **template authors**. It explains how to declare a *field
policy* in your `stacker.yml` so that every buyer of your template gets their own
freshly generated secrets — instead of your development values.

If you publish to the marketplace, this is also what the publish check enforces:
a submission is **rejected** until every secret-shaped field has a policy.

---

## Why you need it

Your `stacker.yml` / compose almost certainly contains values like `JWT_SECRET`,
`POSTGRES_PASSWORD`, or `DASHBOARD_PASSWORD`. Those are *your* development values.
Without a field policy they would be copied verbatim into every buyer's
deployment — so every buyer, and you, would share the same secrets. That is a
security problem the moment more than one person installs your template.

A field policy tells the platform **who controls each field's final value**:

- values you want kept constant (a hostname, a log level default),
- values a buyer may override,
- credentials or tokens that the buyer must provide — the author's value is never shipped,
- and **secrets that must be generated fresh, per install** — the buyer never
  sees your value, and no two buyers share one.

Your literal values are never shipped to buyers for generated or provided
fields; generated values are minted by the platform, while provided values
must come from the buyer.

---

## Where it goes

Declare it under `config_contract` in your `stacker.yml`. Fields are grouped by
service — the service name must match the service in your compose:

```yaml
config_contract:
  services:
    <service-name>:        # must match a service in your compose
      fields:
        <ENV_VAR_NAME>:
          mutability: fixed | editable | provided | generated
          # ...type/constraints depending on mutability
```

---

## `mutability` — who controls the value

| `mutability` | Meaning | Use for |
|---|---|---|
| `fixed` | Your value is baked in; the buyer never changes it. | Constants: internal hostnames, ports, feature flags. |
| `editable` | Your value is a **default**; the buyer may override it. | Tunables: `LOG_LEVEL`, region, replica count. |
| `provided` | The buyer must provide the value; your value is never shipped. | External credentials: AWS access keys, API tokens, private keys. |
| `generated` | The system produces a **fresh value per install**; the buyer never enters it and never sees yours. | Every secret: passwords, API keys, JWT signing keys. |

Fields you don't declare default to `fixed` (today's copy-through behavior) — so
you only *must* declare your secrets, but declaring the rest makes intent explicit.

---

## Generated field `type`s

Every `generated` field needs a `type` that says how to produce the value:

| `type` | Produces | Constraints |
|---|---|---|
| `hex` | random hex string | `length` (characters) |
| `base64` | random base64 string | `length` |
| `alphanumeric` | random `[A-Za-z0-9]` string | `min_length` |
| `uuid` | a UUID v4 | — |
| `enum` | one of a fixed set | `values: [..]` (required) |
| `derived_jwt` | a JWT signed with another field | `signing_key`, `claims`, `alg` |

For `derived_jwt`:

- `signing_key`: `"<service>.<FIELD>"` — a reference to another field (usually a
  `generated` secret) whose resolved value signs this token. It resolves first.
- `claims`: the JWT claims object.
- `alg`: `HS256`, `HS384`, or `HS512` (HMAC).

`editable` fields may also carry `type` + `values` to constrain what a buyer can
enter (e.g. an `enum`).

---

## Worked example

A Supabase-style stack:

```yaml
config_contract:
  services:
    auth:
      fields:
        POSTGRES_HOST:
          mutability: fixed              # constant, shipped as-is
        LOG_LEVEL:
          mutability: editable           # buyer may change; your value is the default
          type: enum
          values: [debug, info, warn, error]
        JWT_SECRET:
          mutability: generated          # fresh per buyer
          type: hex
          length: 32
        DASHBOARD_PASSWORD:
          mutability: generated
          type: alphanumeric
          min_length: 20
    storage:
      fields:
        ANON_KEY:
          mutability: generated
          type: derived_jwt
          signing_key: auth.JWT_SECRET   # signed with this install's JWT_SECRET
          claims: { role: anon, iss: supabase }
          alg: HS256
```

At install time each buyer gets a unique `JWT_SECRET`, a unique
`DASHBOARD_PASSWORD`, and an `ANON_KEY` signed by *their* `JWT_SECRET`.
`POSTGRES_HOST` is constant; `LOG_LEVEL` is `warn` unless the buyer picks another.

---

## The publish requirement

When you submit to the marketplace, the platform scans your template for
secret-shaped environment variables. If any of them lacks a protected
`mutability: generated` or `mutability: provided` policy, the submission is
rejected. Use `generated` for values the platform can create and `provided`
for credentials the buyer must enter.

```
config_contract is missing a `mutability: generated` or `provided` policy for
secret-shaped field(s): <NAMES>. Declare a generator or buyer-provided field
for each in config_contract before publishing.
```

Add a `generated` policy for each listed field and resubmit. This is the single
most common publish rejection related to secrets.

---

## What buyers receive

- `generated` → a fresh, unique value minted for their install (never yours).
- `editable` → the value they chose, or your default if they chose nothing.
- `provided` → the buyer-supplied value; if it is missing, the field is blanked rather than falling back to your value.
- `fixed` → your value, unchanged.

For `provided` fields, the Marketplace deployment form asks the buyer for the
value. TryDirect stores it in the installation-scoped Vault path and removes
the plaintext value from the installation request before it is persisted or
sent through AMQP.

---

## Legacy shorthand (still supported)

Older templates used three plain lists instead of the `fields` map. These still
parse and map as follows, so you don't have to rewrite them immediately:

```yaml
config_contract:
  services:
    auth:
      required: [POSTGRES_HOST]     # -> mutability: fixed, required: true
      optional: [LOG_LEVEL]         # -> mutability: fixed, required: false
      secret:   [JWT_SECRET]        # -> mutability: generated (alphanumeric, min 32)
```

Prefer the `fields` map for new templates — it lets you pick the right `type` and
length per secret.

---

## Authoring workflow

- **Validate** your config before submitting:

  ```bash
  stacker config validate
  ```

- **Local development**: `stacker init` generates a `scripts/generate-secrets.sh`
  from the same policy, so your local runs fill empty secrets the same way the
  marketplace install will — one declared policy drives both.
- Stacker can also **suggest** a starting contract for a stack you've built; run
  `stacker config validate` first and follow its guidance.

---

## Keep real secrets out of your repo anyway

The field policy governs what **buyers** receive — it does not excuse committing
real credentials. Keep secrets in gitignored `.env` files and reference them with
`${VAR}` interpolation. See [MARKETPLACE_PUBLISH.md](./MARKETPLACE_PUBLISH.md)
for the full publishing walkthrough.
