# Backups — dumps in Stacker, storage in a pluggable backend (WS-1)

Status: in progress. 1.1 ✅ (2026-11-24): migration
`20261124142356_create_backup_tables` (`backup_destination`, `backup_job`,
`backup_run`). Crate `crates/backup-core` ✅: dump/restore command builders for
agent-exec (postgres/mysql/mongo/volume, the per-run token only via
`$BACKUP_TOKEN`), retention policy B2 (7 daily + 4 weekly) — tests 9/9 in
~10ms. 1.2-1.6 ahead (db layer, REST, CLI, storage connector, scheduler,
restore e2e).

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

- Dump: `pg_dump -Fc` / `mysqldump` / `mongodump --archive` / `tar | gzip`
  piped into `POST {endpoint}/upload` of the storage backend (device bearer JWT).
- Restore: `GET {endpoint}/upload/files/{id}` → `pg_restore` / `mysql` /
  `mongorestore` / volume unpack.
- Device credentials live only in Vault; the per-run token is passed to the
  command via an environment variable, never in the command line.

## Project rules

- New migrations only; run `cargo sqlx prepare` after changing sqlx queries.
- REST `/api/v1/backups*` needs a casbin migration + `api-reviewer` review.
- `backup.succeeded`/`backup.failed` events go to the WS-N event bus.
