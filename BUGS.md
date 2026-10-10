# Known Bugs — deploy testing

## 1. Cloud deploy fails at compose: `lstat /home/trydirect/.stacker: no such file or directory`

**Status: fix in the working tree (uncommitted).** `reject_build_sections_for_cloud`
(`src/console/commands/cli/deploy.rs`) now stops a cloud deploy that would ship a
`build:` section, before a server is provisioned, and prints the `image:` snippet to
apply. `generated_compose_is_stale` regenerates `.stacker/docker-compose.yml` when
`stacker.yml` is newer, so an artifact predating an added `app.image` is no longer
shipped. `local` and `server` are unchanged.

**This is the real failure.** The cloud-init `rc=2` is a red herring — see §2.

```
TASK [custom : Deployment of Docker services]
fatal: [159.69.114.230]: FAILED! => {
  "cmd": "docker compose -f /home/trydirect/project/docker-compose.yml up -d --remove-orphans",
  "rc": 17,
  "stderr": "resolve : lstat /home/trydirect/.stacker: no such file or directory"
}
PLAY RECAP: ok=151 changed=100 unreachable=0 failed=1 skipped=34 rescued=0 ignored=1
```

### Root cause: the generated compose carries a build context that only resolves locally

`normalize_generated_compose_paths` (`src/console/commands/cli/deploy.rs:583`)
rewrites every generated compose to:

```yaml
build:
  context: ..                        # deploy.rs:649, deploy.rs:657
  dockerfile: .stacker/Dockerfile    # deploy.rs:668
```

That pair is correct **only** when the compose file sits inside
`<project>/.stacker/` — then `..` is the project root and `.stacker/Dockerfile`
resolves under it. The function even guards on that, bailing out unless the path
contains `.stacker` (`deploy.rs:584-590`).

On the server the compose is copied to `/home/trydirect/project/docker-compose.yml`
— the project **root**, not a `.stacker/` subdirectory. So docker compose resolves:

- `context: ..` → `/home/trydirect`
- `dockerfile: .stacker/Dockerfile` → `/home/trydirect/.stacker/Dockerfile`

→ `lstat /home/trydirect/.stacker`, exactly the observed error. One directory
level too high, and `.stacker/` is never uploaded in the first place: the Ansible
log shows only `docker-compose.yml` and `.env` staged
("Staged 2 deploy-time config file(s) before Docker Compose").

The cloud/server bundle (`deploy.rs:3453-3470`) rebases **bind-mount** config
paths for remote layout but leaves `build.context` / `build.dockerfile` untouched.

### Fix direction (stacker-side, this repo)

For `DeployTarget::Cloud | Server`, either:
- rewrite `build.context` → `.` and `dockerfile` → `Dockerfile` for the remote
  layout, and include `.stacker/Dockerfile` in the uploaded bundle; or
- drop `build:` entirely when the service has an `image:` (nothing to build
  remotely — hermes-agent uses `image: nousresearch/hermes-agent:latest`).

## 2. NOT the cause: cloud-init `rc=2`

Ansible's `ignore_errors: yes` handles it — `PLAY RECAP` shows `ignored=1`, and
the play continued through all 151 tasks. `rc=2` is cloud-init's "done,
degraded" from Hetzner's metadata endpoint (`169.254.169.254`) timing out;
stdout is `status: done`.

The deployment pauses because of the `failed=1` in §1, not this.

Also verified: stacker does not parse Ansible output at all. The RabbitMQ
listener (`src/console/commands/mq/listener.rs:459`) reads `msg.status` off the
`ProgressMessage` envelope and writes it to the deployment row
(`listener.rs:505`) — no `FAILED!` matching anywhere. The status write is
unguarded, so a later `completed` would be recorded if one were sent.

## 3. `stacker resolve -y` cannot recover these

`force_complete_deployment` (`src/console/commands/cli/resolve.rs`,
`src/routes/deployment/force_complete.rs`) is a status flip only, by design —
it never re-runs deploy steps, so containers stay down. No partial resume
exists: `InstallServiceConnector` (`src/connectors/install_service/mod.rs:37`)
exposes only `deploy`, `configure_cloud_firewall`, `post_deploy_clone`.

**Affected deployments:** #272–#275, #279–#294.
**Stacker CLI:** v0.3.2 · **Backend:** dev.try.direct

---

# Known Bugs — marketplace

## 4. Deleting a template leaves an orphaned catalog row in User Service (found 2026-09-22)

**Status:** 🔴 OPEN — data is inconsistent in production right now.

Five marketplace entries are reachable in the storefront but 404 on their own
detail data. `GET /api/templates/<slug>` answers `Template '<slug>' not found`
for all of them, filling the blog container log:

```
ghost, vaultwarden, supabase, ai-knowledge-base, ai-workflows-v2
```

The templates table currently holds twelve slugs, and none of these is among
them:

```
n8n-official-release, stackpilot, private-sovereign-ai, localai, komga,
filebrowser, floci, astrbot, uptime-kuma-in-one-click, archivebox,
stackdog, openclaw-preconfigured
```

### The rows still exist on the User Service side

`GET /server/user/catalog/<slug>` returns each of the five with
`kind: "template"`, `source: "marketplace"` and an `extra_data.stack_template_id`
UUID pointing at a template that no longer exists:

| catalog slug | `stack_template_id` | in templates table |
|---|---|---|
| `ghost` | `1f7aea86-eb08-4cf8-a78c-ce21970d39d7` | no |
| `vaultwarden` | `69ab3006-4883-42cf-9240-2ac2093b1780` | no |
| `supabase` | `5fba20e5-a73b-4a63-9ec1-1b52fae01f30` | no |
| `ai-knowledge-base` | `0c30e322-5040-4326-a050-6756ff664209` | no |
| `ai-workflows-v2` | `243e6e9f-b089-44e5-a70d-b79390829f28` | no |
| `n8n-official-release` | `952f196e-36b7-4f01-9f17-46694cd84d89` | **yes** — control case |

So the template was removed (or never finished publishing) on this side while
the product/catalog row it created in User Service (`app/marketplace/views.py`)
survived. Nothing cascades.

### Effect

`/applications/marketplace/<slug>` still renders — blog wraps the enrichment
call in a try/catch — but silently without `stack_definition`, so the page
shows no apps, no version and no vendor. To a buyer it looks like a broken
listing rather than a delisted one.

### Notes for whoever picks this up

- The detail endpoint is **slug-only**. `GET /api/templates/<uuid>` 404s even
  for `n8n-official-release`, whose UUID is valid — so a UUID-based fallback in
  the caller is not an option; the fix has to be in the data.
- Two things are needed: a one-off cleanup of the five stale catalog rows, and
  a cascade (or a periodic reconciliation) so template deletion removes the
  product row it created.

---

# Known Bugs — CLI preflight & deploy (found 2026-10-02, build `ea01dbb`)

Re-verification sweep from the template QA campaign. Full log in
`stacker-project-examples/BUGS.md`. Builds before `ea01dbb6` also had the
`cap_add`/`logs`/`destroy`/stale-lock bugs — those four are fixed and are NOT
repeated here. Each entry below carries its own status; fixed entries keep
their root cause and repro for history.

## 5. ✅ FIXED — Port preflight false-positives on EVERY range mapping

**Severity:** High — blocks `stacker deploy --target local` for any template
using a port range. **File:** `src/cli/install_runner.rs:642-682`
(`check_local_host_port_conflicts`), probe at `:655-660`.
**Status:** fixed in `0083b56e` (10 unit tests) and verified end-to-end
2026-10-03 — see e2e note below.

### Repro (minimal, verified on `ea01dbb`)

```yaml
# stacker.yml
name: t2
app:
  type: custom
  image: nginx:alpine
  ports: ["8100-8105:8100-8105"]
```

```console
$ stacker deploy --target local
Error: Deployment to local failed: Host port conflict detected before deploy:
  • port 8100-8105 (service 'app') is already allocated on this host —
    find the owner with: lsof -nP -iTCP:8100-8105 -sTCP:LISTEN
```

Every port in the range is free (`lsof` empty); the deploy is impossible.

### Root cause

`parse_compose_host_port` (`install_runner.rs:377`) returns the raw host-side
substring — the literal string `8100-8105`. The conflict probe then does:

```rust
// install_runner.rs:655-660
.filter(|(port, _)| {
    let addr = format!("0.0.0.0:{}", port);   // "0.0.0.0:8100-8105" — invalid
    TcpListener::bind(&addr).is_err()          // always Err ⇒ "occupied"
})
```

An unparseable address is indistinguishable from an occupied port: `is_err()`
is treated as "someone holds this port", so **every** range reports a conflict.

### Fix (`0083b56e`)

Expand `start-end` host specs into individual ports before probing (both the
`8100-8105` and mixed `8100-8105:8100-8105` forms), or skip the TCP probe for
specs the parser does not understand and let Docker's own bind error surface
(prefer expansion — the preflight exists to beat Docker's opaque error).
Unit-test territory: `install_runner.rs:4051` already covered a range string
for `parse_compose_host_port` itself — the probe itself now has coverage too
(range expansion, variable resolution, and false-positive regressions for
both repros here, plus a real occupant inside a range still being reported
by name).

## 6. ✅ FIXED — Port preflight cannot parse `${VAR:-default}` — same false conflict

**Severity:** High — blocks every `deploy.compose_file:` stack whose third-party
compose uses default-expansion ports. **File:** same probe as §5.
**Status:** fixed in `0083b56e` (same normalization as §5) and verified
end-to-end 2026-10-03 — see e2e note below.

### Repro (minimal, verified on `ea01dbb`)

```yaml
# stacker.yml
name: t5
app:
  type: custom
  image: nginx:alpine
deploy:
  target: local
  compose_file: ./compose.yml
```

```yaml
# compose.yml
services:
  app:
    image: nginx:alpine
    ports:
      - "${APP_PORT:-7130}:7130"
```

```console
$ stacker deploy --target local
Error: Deployment to local failed: Host port conflict detected before deploy:
  • port -7130} (service 'app') is already allocated on this host
```

### Root cause

Identical mechanism to §5: `parse_compose_host_port` splits `"${APP_PORT:-7130}:7130"`
on `:` and takes the second-to-last segment → `-7130}`. The probe binds
`0.0.0.0:-7130}` → always errors → reported occupied. The port is free; the
string was never a port.

### Fix (`0083b56e`)

Resolve `${VAR}` / `${VAR:-default}` / `${VAR-default}` in the host-port string
before probing — mirror compose's own substitution (env file + process env).
Compose already treats these forms as valid (`generator/compose.rs:1116`
explicitly preserves them in `environment:`), so preflight must too. Shared
helper with §5: `normalize_host_port_spec(raw, env) -> Vec<u16>` used by both
`check_local_host_port_conflicts` and `check_remote_host_port_conflicts`
(`install_runner.rs:516` — the remote probe has the same parser, so the same
false positives fire there: a `${...}` port simply never matches the remote
`occupied` set and silently disables the check).

### e2e verification (2026-10-03, `23735658`)

Both repros re-run as throwaway local projects (not in `stacker-projects/`),
ports free beforehand (`lsof` empty):

```console
$ stacker-cli deploy --target local          # t2: ports ["8100-8105:8100-8105"]
EXIT=0   # no "Host port conflict detected"; t2-app-1 Up, range published

$ stacker-cli deploy --target local          # t5: "${APP_PORT:-7130}:7130"
EXIT=0   # no conflict; t5-app-1 Up, 0.0.0.0:7130->7130/tcp
```

Both stacks were destroyed afterwards (`stacker destroy --confirm --volumes`),
no containers left behind.

## 7. ✅ FIXED — `--target server` reports success while the remote container never starts

**Severity:** High — silent deploy failure, poisons QA records
(`*_DEPLOY_SUCCESS.md` written for dead stacks).
**Status:** fixed in code (stacker `23735658` + install `de2e64b` + contract
`b0ea15e`) and verified end-to-end on 2026-10-03.

### Repro (verified on `ea01dbb`, 2026-10-02)

Shared test box has `gitlab-app-1` on `0.0.0.0:8082`. Deploy `dashy`
(`ports: ["8082:8080"]`):

```console
$ stacker deploy --target server
  Deploying project 'dashy' to 46.224.127.228 via Stacker server...
  Deployment context saved to .stacker/deployment-server.lock
$ echo $?
0
$ ssh root@46.224.127.228 'docker inspect project-app-1 --format "{{.State.Status}} {{.State.Error}}"'
created failed to set up container networking: ... Bind for :::8082 failed: port is already allocated
```

### e2e verification (2026-10-03, `23735658`, backend dev.try.direct)

Same box, same conflict (`gitlab-app-1` still holds 8082), non-TTY stderr
(piped to a file — the exact case that used to print nothing):

```console
$ stacker-cli deploy --target server --server-host 46.224.127.228 \
    --server-user root --server-ssh-key .../stacker-project-test
  ✓ Server deployment requested via Stacker server (project='dashy', project_id=305, deployment_id=416); server='dashy-server'
  Deployment context saved to .stacker/deployment-server.lock

  ✗ Deployment #416 ended as 'paused' [port_conflict]
    A port this stack needs is already in use on the target host (often by another already-deployed stack, e.g. statuspanel). Free the port or change it in stacker.yml, then redeploy.
Error: Deployment to server failed: Deployment #416 ended as 'paused' [port_conflict] ...
$ echo $?
1
$ ssh root@46.224.127.228 'docker inspect project-app-1 --format "{{.State.Status}}|{{.State.Error}}"'
created|failed to set up container networking: ... Bind for 0.0.0.0:8082 failed: port is already allocated
```

All four behaviours are observable: **exit 1**, one verdict line (not two),
the typed `error_kind` surfaced with its remediation text, and the container
state matching the verdict. The platform path is the one taken
(`project_id=305`), so this is the code path the bug lived in.

### Root cause (corrected 2026-10-03)

The original analysis below this heading said the platform path "never waits
for the install job". That was wrong — it **does** wait. Three separate
defects, not one:

1. **The watch verdict was computed and then thrown away.**
   `watch_cloud_deployment` (`deploy.rs`) polls deployment status to a
   terminal state and *correctly* returned `DeploymentWatchOutcome::Failed`.
   The consumer read that value only to decide whether to skip lock claiming
   (`matches!(watch_outcome, DeploymentWatchOutcome::Failed(_))` at
   `:4040` / `:4046`) and then fell through to an unconditional
   `deploy_notify(true)` + `Ok(())` at `:4069-4074`. Exit code 0.
2. **Non-TTY runs had no verdict at all.** The spinner is hidden when stderr
   is a pipe (the QA harness), so even a correct failure message would have
   been invisible.
3. **The Install Service classified already-truncated text.** The producer
   ran `classify_error` on the *first* 1500 bytes of the Ansible/docker
   report. The docker bind signature (`Bind for ... failed: port is already
   allocated`) sits at the *tail* of the report, so dashy was classified as a
   generic `internal_error` before the CLI ever saw it — and the typed
   `available_options` record the API already carried was dropped by serde
   because `ProgressMessage` never declared the field.

### Fix

**stacker** (this commit):

- `DeploymentWatchOutcome` is now `Completed | Failed(Box<DeploymentFailure>) | TimedOut | Unknown`
  (non-`Copy`, so a `Failed` verdict cannot be silently matched-and-dropped).
  `DeploymentFailure` carries `deployment_id`, `status`, `status_message`,
  `error_kind`, `err_description` and renders `summary()` / `reason()`.
- The consumer returns `Err(CliError::DeployFailed { target, reason })` →
  **exit 1** for `Failed` *and* `TimedOut`, and calls `deploy_notify(false)`.
  `Completed` and `Unknown` keep the old success behaviour (an unobservable
  deploy must not be failed).
- `progress::finish_*` echo through `echo_fallback()` when the spinner is
  hidden and stderr is not a TTY, so CI/QA sees the `✗` verdict.
- `ProgressMessage.available_options` (`#[serde(default)]`) plus
  `DeploymentStatusResponse` / `DeploymentStatusInfo` `error_kind` and
  `err_description`, extracted from deployment metadata.
- `DeploymentFailure::from_status_info` prefers `error_kind` and falls back to
  `detect_port_conflicts_in_output` over the status message, so pre-contract
  payloads still classify `port_conflict`.

**install** (commit `de2e64b`): `classify_report` reads `raw_error` (the
untouched report) instead of the head-truncated `out`, and the failure payload
is truncated with `truncate_keeping_ends` so the tail survives.
8 `error_kind` values, `report_on_fail` / `report_on_fail_nostd` both publish
`available_options`.

**config** (commit `b0ea15e`, branch `feature/deploy-failure-contract`):
`shared-fixtures/deploy-failure-payload.json` is the canonical contract;
mirrored to `tests/contracts/deploy-failure-payload.contract.json` here and
enforced by `tests/deploy_failure_payload_contract.rs` (6 tests).

### Remaining soft-spot

The remote preflight returns `vec![]` on any SSH failure
(`install_runner.rs`) — best-effort only. The watch verdict is now the hard
safety net.

## 8. 🟠 OPEN — `./file` bind mounts resolve against `.stacker/`, silently becoming directories

**Severity:** High — every local deploy of a template with a file bind breaks
in a way that looks like an app bug. **File:** generated compose lives in
`.stacker/`, no staging step exists.

### Repro (verified on `ea01dbb`)

```yaml
# project root contains config.yml
app:
  type: custom
  image: nginx:alpine
  ports: ["8896:80"]
  volumes:
    - ./config.yml:/etc/nginx/conf.d/conf.yml:ro
```

```console
$ stacker deploy --target local     # exit 0
$ docker inspect t6-app-1 --format '{{range .Mounts}}{{.Source}}{{"\n"}}{{end}}'
/private/tmp/buginz/t6/.stacker/config.yml      # ← project root never consulted
$ docker exec t6-app-1 cat /etc/nginx/conf.d/conf.yml
cat: read error: Is a directory                 # docker auto-created a dir
```

### Root cause

Docker resolves relative bind sources against the **compose file's directory**
(`.stacker/`), not the project root. The config-bundle pipeline stages files
for remote deploys (`Config file: config.yml -> config.yml` appears in deploy
output) but nothing stages them for local runs; `.stacker/config.yml` doesn't
exist, so docker creates an empty directory in its place — no error anywhere.

### Fix direction

One behavior for local and remote, pick one:
- run local compose with `--project-directory <project root>` (compose flag,
  no file churn), or
- rewrite `./x` → `../x` in `normalize_generated_compose_paths` (it already
  rewrites `build.context` the same way — see §deploy-1 at top of this file),
  or
- stage bind sources into `.stacker/` during render (mirrors the remote bundle).

## 9. 🟠 OPEN — `app.dockerfile: Dockerfile` is rewritten to a file that is never generated

**Severity:** High — `stacker deploy` fails at build for the most natural
spelling of "my Dockerfile is in the project root". **File:**
`src/console/commands/cli/deploy.rs` (`normalize_generated_compose_paths`,
~`:661-670`).

### Repro (verified on `ea01dbb`)

```yaml
# Dockerfile exists in project root
app:
  type: custom
  dockerfile: Dockerfile
```

```console
$ stacker deploy --target local --dry-run --force-rebuild
  App build source: context=..., dockerfile=.../.stacker/../.stacker/Dockerfile
$ grep -A2 build .stacker/docker-compose.yml
    context: ..
    dockerfile: .stacker/Dockerfile        # ← does not exist
$ ls .stacker/
docker-compose.yml                          # ← no Dockerfile, by design
```

### Root cause

The normalizer unconditionally rewrites `dockerfile: Dockerfile` (and
`./Dockerfile`, and `None`) → `.stacker/Dockerfile`. But when
`app.dockerfile` is set explicitly, the generator deliberately does NOT
create `.stacker/Dockerfile` — asserted by the test at `deploy.rs:5591`
("Custom Dockerfile should not be overwritten / .stacker/Dockerfile should NOT
be generated"). The rewrite contradicts the generator's own invariant.

### Fix direction

Skip the `dockerfile:` rewrite when `config.app.dockerfile` is set (rewrite to
the configured path relative to the compose location instead), or drop the
rewrite entirely when the target path doesn't exist on disk. Workaround in
use: rename to `Dockerfile.custom` (rewrite only matches
`None | "Dockerfile" | "./Dockerfile"`).

## 10. 🟡 OPEN — `AppSource` accepts unknown fields silently

**Severity:** Medium — the whole `cap_add`/`privileged`/`platform` class of bug
keeps recurring; each new unknown key is silent config loss. **File:**
`src/cli/config_parser.rs:196` (`AppSource`), service struct at `:313`.

### Repro (verified on `ea01dbb`)

```yaml
app:
  type: custom
  image: nginx:alpine
  totally_unknown_key: true
```

```console
$ stacker config validate
✓ Configuration is valid        # key is dropped without a word
```

### Root cause

`AppSource` (and `ServiceDefinition`'s source struct) derive plain
`Deserialize` with no `deny_unknown_fields`. The contract structs DO deny
(`ConfigContract` at `:919`, `RawFieldPolicy` at `:1000`,
`RawTargetConfigContract` at `:1295`) — so strictness exists in the codebase,
it just never reached `app:`/`services[]`. Known instances of the resulting
loss: `app.platform`, `app.privileged` (fixed `ea01dbb6` alongside `cap_add`),
and anything the generator forgets tomorrow.

### Fix direction

Add `#[serde(deny_unknown_fields)]` to `AppSource` and the service source
struct. Expected fallout: templates in
`stacker-project-examples/stacker-projects/` carrying junk keys will start
failing validation — that is the point (loud now, broken later otherwise).
If a softer landing is wanted, emit a `W00x: unknown key 'x' ignored` warning
from a pre-pass over the raw YAML first, then turn on the hard error after the
catalog is clean.
