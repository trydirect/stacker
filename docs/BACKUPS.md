# Backups — dumps in Stacker, storage in a pluggable backend (WS-1)

Status: in progress. 1.1 ✅ (2026-11-24): migration
`20261124142356_create_backup_tables` (`backup_destination`, `backup_job`,
`backup_run`). Crate `crates/backup-core` ✅: typed `DumpPlan`/`RestorePlan`
(universal upload protocol — `UploadBody` raw|multipart, `Auth`
bearer|client_credentials|basic), retention policy B2 (7 daily + 4 weekly).
1.2 ✅: models + db layer (`src/models/backup.rs`, `src/db/backup.rs`) +
enqueue service (`src/services/backup.rs`) — builds the plan and queues a
`backup` agent command; runtime-tested (`tests/backup_db.rs`). The agent-side
native `backup` command (Status Panel, `status/` repo) is specified in its
`FEATURES.md` (status#144).
1.5 ✅ (2026-11-26): REST `/api/v1/backups/*` (destinations, jobs, runs, trigger)
+ casbin + CLI `stacker backup {list,run,destinations,runs}`. Owner-gated,
vault_path never leaked (masked endpoint only). Run lifecycle: when the agent
reports a `backup` command, the run is marked uploaded/failed and
`backup.succeeded`/`backup.failed` is emitted on the WS-N bus (report handler
→ `notify_backup_result`). Tests: `tests/backup_rest.rs`, security matrix
(route classification + live IDOR).
1.4 ✅ (2026-11-26): background scheduler (`src/services/backup_scheduler.rs`)
— every minute it finds due jobs (next cron time after the last run has
arrived; standard 5-field schedules supported), enqueues a `backup` command
via the project's deployment, and applies retention (B2: 7 daily + 4 weekly →
mark pruned). Cron matching via the `cron` crate.
1.3 ✅ (2026-11-26): storage connector (`src/services/storage.rs`) —
`StorageClient` with OAuth2 `client_credentials` auth (token cached, refreshed
on 401), retry on 5xx/timeout, and the control-plane ops Stacker needs:
`register_device` (`POST /connect`), `delete_file` (retention cleanup),
`list_files`. Wired into scheduler retention (pruned archives are deleted from
storage). The heavy data path (upload/download) stays agent-side. Tests:
wiremock (4). 1.6 ahead (restore e2e).
BDD ✅: `tests/features/backups.feature` (6 acceptance scenarios — create
destination/job, list, vault_path never leaked, invalid source rejected,
cross-user read/delete refused) + `tests/steps/backup.rs`, wired into the
cucumber harness (`cargo test --test bdd`, 340 scenarios / 1847 steps green).

A `backup` agent command carries `{action, run_id, plan}` where `plan` is a
`DumpPlan`/`RestorePlan`. The command queue stores only the token env var
*name* (`BACKUP_TOKEN`), never a secret value.

The storage backend is pluggable: today a REST backup-storage service, tomorrow
anything with a compatible API (e.g. a restic-based client). Nothing
vendor-specific is hardcoded — upload header names and endpoints come from
`backup_destination` configuration.

Plan: `config/PLAN-backups.md`.
Contract: `config/shared-fixtures/api-contracts/backups-storage.json`.
BDD scenarios: `config/shared-fixtures/features/backups.feature`.

## Decisions (2026-11-24)

- **B1:** the dump runs on the target server through the existing agent
  command queue (a native agent `backup` command is the target end state;
  the command-queue path needs no agent changes).
- **B2:** retention — 7 daily + 4 weekly copies.
- **B3:** storage-side hardening is an external task owned by the storage
  project; until it is closed, the integration is not enabled in production.

## Architecture

Pure logic lives in the leaf crate `crates/backup-core` (commands + retention).
The glue (Vault, db, scheduler, REST) lives in the monolith.

A dump/restore plan is a process `argv` plus an `UploadTarget`/`DownloadTarget`
that **describes the storage protocol** (body encoding + auth + headers) — so
the backend is pluggable (a REST backup-storage service today, a restic/S3
target tomorrow):

- `UploadBody`: `raw` (stream is the body) or `multipart` (archive in a named
  field + literal fields, e.g. `device_id=$AGENT_ID`).
- `Auth`: `none`, `bearer`, `client_credentials` (OAuth2 service account), or
  `basic`. Credentials are referenced by env-var *name*, never carried as
  values in the plan.
- The agent interprets the plan generically (runs the process, streams the
  archive to the backend with its native HTTP client).

Dump: `pg_dump -Fc` / `mysqldump` / `mongodump --archive` / `tar` (volume).
Restore: stream the stored archive into `pg_restore` / `mysql` /
`mongorestore` / volume unpack.

## Project rules

- New migrations only; run `cargo sqlx prepare` after changing sqlx queries.
- REST `/api/v1/backups*` needs a casbin migration + `api-reviewer` review.
- `backup.succeeded`/`backup.failed` events go to the WS-N event bus.

## Restore runbook (WS-1 1.6)

Restore is "the chosen backup run" (latest by default). It re-streams a stored
archive into the target via the agent. Point-in-time restore does not exist on
the storage side (current state only).

**1. Find the run to restore.** List a job's runs and pick one (the `remoteFileId`
is the archive in storage):

```
stacker backup runs --job <job_id>
# or: GET /api/v1/backups/jobs/<job_id>/runs
```

**2. Trigger the restore.** Stacker queues a `backup` restore command for the
project's agent:

```
stacker backup restore --run <run_id>
# or: POST /api/v1/backups/runs/<run_id>/restore
```

**3. The agent executes it** (the native `backup` command, `status#144`): it
streams `GET {endpoint}/upload/files/{remoteFileId}` into the restore process
(`pg_restore` / `mysql` / `mongorestore` / volume unpack). It reports the
result back keyed by `run_id`.

**4. Verify.** Check the run's status (`stacker backup runs --job <job_id>`)
and the application data. A `backup.succeeded`/`backup.failed` event is
emitted on completion and delivered to the project's notification channels.

**Notes**
- Restore targets the project's current deployment (its container/volume).
  Restoring into a different server is out of scope.
- If `BACKUP_TOKEN_URL` / the destination credentials are not configured, the
  trigger answers an error instead of queuing.
- For a full-DB restore, put the app in maintenance / stop writers first to
  avoid a torn state.
