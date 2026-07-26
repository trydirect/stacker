# Audit Checker Endpoints — Manual Testing Guide

Six public, unauthenticated checkers under `/api/audit/*`, backed by the pure
`td-audit` crate. This guide shows how to exercise every endpoint by hand.

## 0. Prerequisites

```bash
# From the stacker worktree:
SQLX_OFFLINE=true cargo build --bin server

# Apply the casbin grant so anonymous users can reach /api/audit/* :
sqlx migrate run            # includes 20260726120000_casbin_audit_public_rules

# Run the server (needs Postgres + config as usual)
cargo run --bin server
```

Set your base URL (adjust the port to your config):

```bash
export BASE=http://localhost:8000
```

All checkers are **anonymous** — no `Authorization` header needed. The
paste-based checkers take the file as a **raw text body**
(`Content-Type: text/plain`); `image` takes a small JSON body.

Handy fixtures live in `config/shared-fixtures/audit/`.

---

## 1. Compose Auditor — `POST /api/audit/compose`

```bash
curl -sS -X POST "$BASE/api/audit/compose" \
  -H 'Content-Type: text/plain' \
  --data-binary @$HOME/work/try.direct/config/shared-fixtures/audit/compose/insecure.yml | jq
```

Expect a low grade with a critical `compose.no_secrets` finding:

```json
{
  "checker": "compose",
  "score": 50,
  "grade": "F",
  "summary": "1 critical, 1 info",
  "findings": [
    { "id": "compose.no_secrets", "severity": "critical", "title": "…", "detail": "…", "remediation": "…" }
  ],
  "cta": { "label": "Fix these and deploy on TryDirect →", "url": "https://try.direct/deploy" }
}
```

Clean input → grade `A`:

```bash
curl -sS -X POST "$BASE/api/audit/compose" -H 'Content-Type: text/plain' \
  --data-binary @$HOME/work/try.direct/config/shared-fixtures/audit/compose/clean.yml | jq '.grade'
```

---

## 2. Dockerfile Linter — `POST /api/audit/dockerfile`

```bash
curl -sS -X POST "$BASE/api/audit/dockerfile" -H 'Content-Type: text/plain' \
  --data-binary @$HOME/work/try.direct/config/shared-fixtures/audit/dockerfile/unpinned.Dockerfile | jq '.findings[].id'
```

Expect `dockerfile.unpinned_base`, `dockerfile.root_user`, `dockerfile.no_healthcheck`.
Inline secret example → critical `dockerfile.secret_in_env`:

```bash
printf 'FROM alpine:3.20\nUSER app\nENV API_KEY=abcd1234efgh5678ijkl\n' \
 | curl -sS -X POST "$BASE/api/audit/dockerfile" -H 'Content-Type: text/plain' --data-binary @- | jq
```

---

## 3. Exposure Audit — `POST /api/audit/exposure`

```bash
curl -sS -X POST "$BASE/api/audit/exposure" -H 'Content-Type: text/plain' \
  --data-binary @$HOME/work/try.direct/config/shared-fixtures/audit/compose/public-db.yml | jq
```

Expect grade `F` with critical `exposure.sensitive_port_public` (Postgres on 0.0.0.0).
A loopback-bound DB (`clean.yml`) returns no findings.

---

## 4. Readiness Score — `POST /api/audit/readiness`

```bash
curl -sS -X POST "$BASE/api/audit/readiness" -H 'Content-Type: text/plain' \
  --data-binary @$HOME/work/try.direct/config/shared-fixtures/audit/compose/public-db.yml | jq '.findings[].id'
```

Aggregates compose security + exposure + operational gaps
(`readiness.no_restart_policy`, `readiness.no_healthcheck`, `readiness.no_memory_limit`).

---

## 5. Cost & Sizing Estimator — `POST /api/audit/cost`

```bash
curl -sS -X POST "$BASE/api/audit/cost" -H 'Content-Type: text/plain' \
  --data-binary @$HOME/work/try.direct/config/shared-fixtures/audit/compose/clean.yml | jq
```

Returns sizing + cheapest fitting instance per provider (ascending), e.g.:

```json
{
  "sizing": { "service_count": 1, "total_cpus": 1.0, "total_memory_mb": 640 },
  "quotes": [ { "provider": "hetzner", "instance": "cpx11", "monthly_usd": 4.35, "…": "…" } ],
  "cheapest": { "provider": "hetzner", "instance": "cpx11", "monthly_usd": 4.35 },
  "priced_as_of": "2026-07"
}
```

---

## 6. Image Inspector — `POST /api/audit/image`

JSON body with an image reference:

```bash
# Missing image -> critical image.not_found, grade F
curl -sS -X POST "$BASE/api/audit/image" -H 'Content-Type: application/json' \
  -d '{"image":"trydirect/does-not-exist:latest"}' | jq

# Official pinned image -> grade A
curl -sS -X POST "$BASE/api/audit/image" -H 'Content-Type: application/json' \
  -d '{"image":"library/redis:7-alpine"}' | jq '.grade'

# Unpinned tag -> warning image.unpinned_tag
curl -sS -X POST "$BASE/api/audit/image" -H 'Content-Type: application/json' \
  -d '{"image":"nginx:latest"}' | jq '.findings[].id'
```

Notes:
- Only ever contacts `hub.docker.com` (no SSRF surface).
- CVE scanning (Trivy/Grype via the `VulnScanner` trait) is a follow-up; grades
  currently reflect metadata only (existence, pinning, publisher, staleness).

---

## Pure-engine tests (no server needed)

The engines are fully covered without HTTP/DB:

```bash
cargo test -p td-audit          # 76 unit tests + 9 BDD scenarios (cucumber)
cargo test -p td-audit --test bdd
```
