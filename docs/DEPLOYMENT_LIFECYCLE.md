# Deployment lifecycle & agent identity

What a deployment is, what creates one, what mutates one, and how commands
find theirs. This is the model the code implements; the security properties
behind it live in [APP_DEPLOYMENT.md](APP_DEPLOYMENT.md), the operator
commands in [CHEATSHEET.md](CHEATSHEET.md).

Written because the model was spread across behaviour and bug records
(BUGS.md #51, #55, #57, #59, #61) and never stated in one place — which is
how a reinstalled agent ends up invisible to `stacker agent status`.

## What a deployment is

A deployment is the **identity and isolation unit of one release**. Every
moving part is keyed by the same `deployment_hash`:

| Piece | Keyed by | Where |
|---|---|---|
| Secret scope | `{prefix}/{deployment_hash}/*` | `docs/APP_DEPLOYMENT.md` "Vault Path Structure" |
| Agent token (`status_panel_token`, TTL 30 days) | issued per deployment | `docs/APP_DEPLOYMENT.md` "Token Provisioning" |
| Vault policy | `status-panel-{deployment_hash}` | `docs/APP_DEPLOYMENT.md` |
| Terraform workspace + state + plan file | workspace named `deployment_hash` | install service, `app/tfa/Tofu.py:117-118` |
| Agent registration | belongs to a deployment, not to a server | `src/routes/agent/link.rs:34-40` |

Because the agent's credentials and secret scope are per deployment, "the
agent belongs to a deployment" is not bureaucracy — moving a command to a
different deployment moves it to a different security boundary.

The infrastructure side is deliberately **immutable per release**: each run
generates a fresh plan against its own workspace (`tofu plan -out
<hash>.plan`, `Tofu.py:82-106`), a redeploy deletes the workspace first
(`Tofu.py:94-101`), and a successful run deletes it after (`Tofu.py:38-45`).
A plan file is bound to its workspace's state and cannot be replayed against
another deployment — deployments are snapshots, not convergent state. That
is also why each deployment is a rollback point.

## What creates a deployment — and what does not

| Action | Creates a deployment? | Notes |
|---|---|---|
| `stacker deploy` | **yes** — a new release | new hash, new agent registration, own Vault scope; rollback points come from this |
| `stacker agent install` | **yes, today** | known gap: the install deploys the status panel as a *new* deployment, so the hash changes underfoot (BUGS.md #59). Direction: when a deployment already exists, re-register the agent on it (`POST /api/v1/agent/link` already "creates or returns an agent", `src/routes/agent/link.rs:89-127`) instead of deploying again |
| `stacker agent deploy-app` / future `agent add/update/remove` | no | mutates the apps of the existing deployment via agent commands (`agent.rs:874` `deploy_app` enqueue) |
| agent re-registration (`/api/v1/agent/link`) | no | idempotent on the deployment: reuses the agent record, re-issues its token |
| `stacker deployment use <hash>` | no | pins `deploy.deployment_hash` in `stacker.yml` |

## How commands find their deployment

Resolution order (BUGS.md #55, implemented):

1. `--deployment <hash>` — explicit, one command;
2. `deploy.deployment_hash` in `stacker.yml` — the pin, written by
   `stacker deployment use <hash>`, cleared by `--clear`;
3. the lock of the active deploy target (`.stacker/deployment-*.lock`);
4. the newest lock.

Every implicit resolution prints what it picked and where it read that from
(`resolved deployment <hash> (from …)`). If a newer lock exists than the one
chosen, the command warns with both dates.

**Who writes the lock:** a deploy writes it as a side effect. `agent install`
does not — which is exactly how a freshly installed agent becomes invisible:
the install created deployment *N+1* while the lock kept pointing at *N*, so
the next `stacker agent status` queried a deployment whose agent no longer
runs (BUGS.md #59). Until that is fixed, the rule is: after `agent install`
or `deploy`, take the newest hash from `stacker deployments` and pin it.

**Agent status and the heartbeat rule.** An agent is `offline` when its
heartbeat is older than 5 minutes, or when it never reported
(`src/routes/agent/snapshot.rs:531-543`). Commands sent to a deployment with
an offline agent sit in `queued` until the CLI's poll deadline (60 s
default, 300 s for `deploy-app`) — nobody is polling to pick them up.

## Landed (2026-11-23)

The gaps this section tracked are closed (details and verify steps in
BUGS.md #57, #59, #60, #61):

* `agent install` records the deployment it creates — and when the project
  already has one, it **re-registers the agent on it** (`agent link`)
  instead of deploying a new release; `--new-deployment` forces the old
  path.
* A command whose agent provably cannot pick it up is refused before
  enqueueing, with the fix line; a slow but alive agent keeps its full wait
  window.
* A command the agent refuses reports the agent's own words.

Still open, and it belongs to the status panel agent: the platform and the
agent disagree on the `check_connections` command list (BUGS.md #60).

## Related

* [APP_DEPLOYMENT.md](APP_DEPLOYMENT.md) — secret isolation, Vault paths,
  token provisioning.
* [CHEATSHEET.md](CHEATSHEET.md) — `deployment use`, `--deployment`,
  resolution order.
* [SECRET_LIFECYCLE.md](SECRET_LIFECYCLE.md) — who touches a value at each
  stage (install service = Terraform + Ansible).
* `blog/docs/STREAM-stacker-end-to-end.md` — the operator workaround this
  model currently requires.
