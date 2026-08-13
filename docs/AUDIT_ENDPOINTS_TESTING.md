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

## Rate limiting

`/api/audit/*` is protected by a Redis-backed limiter (shared across replicas),
fixed 60s windows:

| Guard | Default | Response |
|-------|---------|----------|
| Body size | `AUDIT_MAX_BODY_KB=256` | `413 Payload Too Large` |
| Per-IP, cheap checkers | `AUDIT_RATE_LIMIT_PER_MIN=30` | `429` + `Retry-After` |
| Per-IP, `image` | `AUDIT_IMAGE_RATE_LIMIT_PER_MIN=5` | `429` + `Retry-After` |
| Global ceiling | `AUDIT_GLOBAL_PER_MIN=600` | `503` + `Retry-After` |

Client IP is taken from `X-Forwarded-For` via the proxy, so ensure the edge
proxy sets it (nginx `proxy_set_header X-Forwarded-For …`) and is the only hop
that can. If Redis is unreachable the limiter **fails open** (requests allowed;
body cap still applies).

Observe a 429 (cheap tier, 30/min):

```bash
for i in $(seq 1 32); do
  code=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/api/audit/compose" \
    -H 'Content-Type: text/plain' --data 'services: {}')
  echo "req $i -> $code"
done
# ... first 30 -> 200, then 429 with a Retry-After header.
```

## Result caching

Identical inputs are cached in Redis for `AUDIT_CACHE_TTL_SECS` (default 60s),
keyed by `sha256(body)` per checker — the biggest win for `image` (skips the
Docker Hub / Trivy work on a repeat). Every response carries an `X-Audit-Cache`
header: `MISS` (computed + stored), `HIT` (served from cache), or `BYPASS`
(Redis unavailable). Observe it:

```bash
curl -si -X POST "$BASE/api/audit/compose" -H 'Content-Type: text/plain' \
  --data 'services: {}' | grep -i x-audit-cache      # MISS
curl -si -X POST "$BASE/api/audit/compose" -H 'Content-Type: text/plain' \
  --data 'services: {}' | grep -i x-audit-cache      # HIT
```

## Metrics

Prometheus counters (exposed on the server's `/metrics`):

- `audit_rate_limit_total{tier,decision}` — `tier` ∈ `cheap|image`, `decision` ∈ `allow|throttle|overload`.
- `audit_cache_total{checker,result}` — `result` ∈ `hit|miss|bypass`.

Cache hit-rate = `audit_cache_total{result="hit"} / sum(audit_cache_total)`;
throttle rate = `audit_rate_limit_total{decision!="allow"} / sum(...)`.

## Pure-engine tests (no server needed)

The engines are fully covered without HTTP/DB:

```bash
cargo test -p td-audit          # 76 unit tests + 9 BDD scenarios (cucumber)
cargo test -p td-audit --test bdd
```
