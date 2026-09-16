# Stacker Sync Development Plan

## Decision

`stacker sync` is a developer-oriented project synchronization command.

It synchronizes the local Stacker configuration with the Stacker API, project
database, `project_app` records, and Vault-backed service secrets. It does not
connect to the target server, does not call Install Service, and does not
start or restart containers.

The normal deployment remains responsible for rendering and uploading runtime
files to the target server:

```text
stacker sync
    -> Stacker API / project database
    -> project_app configuration
    -> Vault service secrets

stacker deploy
    -> fresh runtime bundle
    -> Install Service
    -> target server files and containers
```

This keeps the database as the canonical source for non-secret configuration
and Vault as the canonical source for secrets. It avoids storing a complete
deploy bundle, including `.env` secrets, in project metadata.

## Motivation

The command is intended for fast developer iteration and validation of:

- environment variables;
- app and service configuration;
- marketplace assets metadata;
- marketplace seed jobs metadata;
- config contracts;
- generated fields;
- project-to-`project_app` synchronization.

Typical workflow:

```bash
stacker sync --verify
stacker deploy --dry-run
stacker deploy
```

`sync` verifies that the configuration reached Stacker. `deploy --dry-run`
verifies local runtime generation. A real deploy is still required to change
the target server.

## Existing Functionality

### CLI deployment preparation

The current deployment preparation flow is in:

```text
src/console/commands/cli/deploy.rs
src/cli/config_bundle.rs
src/cli/stacker_client.rs
```

It already knows how to:

- parse `stacker.yml`;
- resolve the project identity;
- build project metadata;
- generate Compose;
- generate config bundle manifests;
- collect config files and `.env`;
- calculate checksums;
- attach bundle metadata to the project payload.

This preparation logic should be extracted into shared functions used by both
`deploy` and `sync`.

### Project API

The existing project update route is:

```text
src/routes/project/update.rs
```

It currently:

- validates `ProjectForm`;
- updates `project.metadata`;
- updates `project.request_json`;
- synchronizes project-level `project_app` rows.

The current `PUT /project/{id}` path is not sufficient for sync because it does
not apply the full deployment bundle flow from `routes/project/deploy.rs` and
does not safely handle secret material.

### Project apps

Project app configuration is stored in:

```text
project_app.environment
project_app.ports
project_app.volumes
project_app.config_files
project_app.config_contract
```

Relevant code:

```text
src/project_app/sync.rs
src/services/project_app_service.rs
src/routes/project/app.rs
src/db/project_app.rs
```

`sync_project_level_apps_from_form()` currently writes directly through DB
helpers and does not perform Vault synchronization. The sync implementation
must account for this rather than assuming the existing project update path is
secret-safe.

### Vault and service secrets

Existing Vault-backed service secret APIs are in:

```text
src/routes/project/secret.rs
src/services/project_app_service.rs
src/project_app/vault.rs
src/services/vault_service.rs
```

The existing service secret flow stores the value in Vault and only stores
metadata in PostgreSQL. This is the preferred path for secret values.

### Marketplace artifacts

Marketplace fields are represented in `custom`:

```text
marketplace_config_files
marketplace_assets
marketplace_seed_jobs
marketplace_post_deploy_hooks
```

Runtime artifact metadata is generated in:

```text
src/routes/project/deploy.rs
```

The current metadata includes asset checksums, sizes, download information,
and deferred execution markers. Presigned URLs are temporary and must not be

## Command Interface

Add a top-level CLI command:

```bash
stacker sync
```

Initial options:

```bash
stacker sync --verify
stacker sync --json
stacker sync --deployment <HASH>
stacker sync --env <ENVIRONMENT>
```

Do not add `--force-new`, server selection, SSH options, or runtime restart
options to the first version. The command must not provision infrastructure.

### Default resolution

Resolve the project from `stacker.yml` using the same identity rules as deploy.

Resolve the project target in this order:

1. Explicit `--deployment` hash.
2. `deploy.deployment_hash`, if configured.
3. `deployment_id` from the active deployment lock, resolved through the API.
4. Latest project deployment only as a last compatibility fallback.

The fallback must emit a warning because a project can have several historical
deployments and servers.

## Data Ownership Rules

### Store in PostgreSQL

Store non-secret, declarative configuration:

- app image;
- app name and code;
- non-secret environment values;
- ports;
- volumes;
- domains;
- restart policy;
- config contract;
- non-secret config file templates;
- marketplace asset descriptors;
- marketplace seed job descriptors;
- marketplace version and source metadata;
- config and asset manifests containing checksums and sizes.

Example:

```text
FLOCI_ENDPOINT=http://floci:4566
```

may be stored in `project_app.environment`.

### Store in Vault

Store values such as:

- passwords;
- access keys;
- private keys;
- registry credentials;
- API tokens;
- secret `.env` values;
- credentials embedded in config files.

PostgreSQL should contain only the corresponding remote-secret metadata and
Vault path.

### Never store as permanent project metadata

Do not permanently store:

- the complete raw `.env` file when it contains secrets;
- complete secret-bearing config bundles;
- presigned asset download URLs;
- temporary deployment credentials;
- rendered payloads containing secrets.

## Backend API

Add a dedicated endpoint instead of overloading the existing generic project
update route:

```http
PUT /api/v1/project/{project_id}/sync
```

The endpoint should accept a sync payload containing:

- project form data;
- non-secret app configuration;
- config file manifest;
- non-secret config file contents where allowed;
- asset metadata;
- seed job metadata;
- environment/profile name;
- verification mode;
- an optional client request ID for idempotency.

Secret values must either be sent through the existing service-secret flow or
be rejected with an actionable error. The endpoint must not silently classify
secrets by variable name alone.

### Backend sync sequence

1. Authenticate the user.
2. Verify project ownership.
3. Resolve and validate the requested deployment hash when provided.
4. Validate the project form.
5. Validate app images and config contracts.
6. Upsert project metadata.
7. Upsert project-level `project_app` rows.
8. Persist non-secret environment and config fields.
9. Persist or update Vault-backed service-secret metadata.
10. Validate marketplace asset descriptors and checksums.
11. Persist seed job descriptors without executing them.
12. Return a redacted synchronization summary.

The endpoint must not:

- create a deployment row;
- update deployment status;
- call Install Service;
- provision or inspect a target server;
- execute seed jobs;
- execute post-deploy hooks.

## Secret Handling

The sync endpoint must not copy the entire local `.env` into project metadata.

The recommended processing model is:

1. Parse local environment values.
2. Resolve the app to which each variable belongs.
3. Use the config contract when available.
4. Persist editable, non-secret values to `project_app.environment`.
5. Send secret values through the service-secret/Vault API.
6. Reject secret-shaped undeclared fields instead of guessing.
7. Return only names, counts, and redacted metadata.

The first implementation may support an explicit developer override for known
non-secret fields, but it must not introduce a general-purpose name heuristic
such as treating every `*_KEY` variable as safe or unsafe.

## Marketplace Assets

`sync --verify` should validate asset metadata without downloading assets to the
target server.

For each asset, validate:

- filename;
- storage provider;
- object key;
- checksum;
- size;
- content type;
- mount path;
- fetch target;
- decompress flag;
- executable flag;
- immutable flag.

Use the existing object-storage verification code where possible:

```text
src/services/marketplace_assets.rs
```

Persist asset descriptors and checksums. Generate fresh presigned download URLs
only when a real deployment needs them.

## Marketplace Seed Jobs

`sync` must persist and validate seed jobs, but must never execute them.

Validation should cover:

- job name;
- required fields;
- target service;
- command or payload;
- dependencies;
- duplicate job names;
- asset references;
- supported execution mode;
- invalid or unsafe paths.

The first release does not need a `seed run` command. If execution is added
later, it must be a separate explicit operation:

```bash
stacker seed validate
stacker seed run --job <NAME>
```

## Verification Response

`stacker sync --verify` should verify database and object-storage state, not
remote server state.

Example response:

```json
{
  "operation": "project_sync",
  "status": "completed",
  "project_id": 217,
  "apps_updated": 2,
  "non_secret_variables_updated": 6,
  "secrets_synced": 2,
  "assets_verified": 3,
  "seed_jobs_validated": 4,
  "deployment_created": false,
  "server_contacted": false,
  "containers_started": false
}
```

The output must never include secret values, raw `.env` contents, Vault values,
or presigned URLs.

## CLI Implementation

Modify:

```text
src/bin/stacker.rs
```

Add:

- `StackerCommands::Sync`;
- `--verify`;
- `--json`;
- `--deployment`;
- `--env`;
- command dispatch.

Add:

```text
src/console/commands/cli/sync.rs
```

Responsibilities:

- load and validate `stacker.yml`;
- resolve project identity;
- prepare shared deployment artifacts;
- build the project sync request;
- call the backend sync endpoint;
- optionally poll/verify the response;
- format human and JSON output.

Extract shared artifact preparation from:

src/console/commands/cli/deploy.rs
```

Reuse:

```text
src/cli/config_bundle.rs
src/cli/stacker_client.rs
```

## Project App and Vault Implementation

The current `sync_project_level_apps_from_form()` writes directly to the
database and does not sync Vault. The new sync path must not assume this is
enough.

Choose one of these implementation paths:

1. Refactor project app synchronization to use `ProjectAppService` where Vault
   synchronization is required.
2. Keep the existing non-secret project sync path and explicitly call the
   secret/Vault synchronization service for declared service secrets.

The second option is less invasive for the first release. Add tests proving:

- non-secret values reach `project_app.environment`;
- secret values do not appear in project metadata;
- Vault metadata is updated;
- API responses remain redacted;
- config contract is preserved.

## Idempotency

Repeated sync with the same source configuration should be safe.

Use a deterministic configuration fingerprint based on:

- normalized project metadata;
- app configurations;
- non-secret environment;
- asset descriptors;
- seed job descriptors;
- config contract.

The fingerprint should not include secret values directly. Secret versions or
Vault metadata versions may be used instead.

Return whether the operation was:

```text
created
updated
unchanged
```

## Tests

### CLI tests

Add:

tests/cli_sync.rs
```

Cover:

- command parsing;
- `--verify`;
- `--json`;
- explicit deployment resolution;
- missing project identity;
- missing deployment behavior;
- no server connection;
- no Install Service call.

### Backend tests

Cover:

- owner can sync;
- another user cannot sync;
- non-secret project fields are persisted;
- project apps are updated;
- secrets are routed to Vault;
- raw secrets are absent from project responses;
- assets are verified;
- seed jobs are validated but not executed;
- no deployment row is inserted;
- deployment status is unchanged.

### Regression tests

Add regressions for:

- `FLOCI_ENDPOINT` being persisted as an app environment value;
- Web UI reading the updated value after CLI sync;
- config contracts not being lost during sync;
- marketplace fields surviving project updates;
- presigned URLs not being persisted as permanent configuration;
- repeated sync being idempotent.

## Implementation Stages

### Stage 1: Shared preparation

- Extract project payload and artifact preparation from deploy.
- Add config and marketplace validation helpers.
- Add tests without changing runtime behavior.

### Stage 2: Backend sync endpoint

- Add request/response types.
- Add ownership validation.
- Add project metadata and project app synchronization.
- Add redacted response.

### Stage 3: Secret handling

- Integrate declared service secrets with Vault.
- Prevent raw `.env` persistence.
- Fix or bypass direct DB app updates that skip Vault when appropriate.

### Stage 4: Assets and seed jobs

- Add asset verification.
- Add seed job validation.
- Persist descriptors without execution.
- Regenerate presigned URLs only during real deploy.

### Stage 5: CLI command

- Add `stacker sync`.
- Add verification and JSON output.
- Add deployment/project resolution.

### Stage 6: Documentation and rollout

- Document developer workflow.
- Document secret handling.
- Document that sync does not affect running containers.
- Add feature flag if backend and CLI versions may be deployed independently.

## Acceptance Criteria

Given a local project with changed configuration:

```bash
stacker sync --verify
```

must:

- update the project in Stack Builder;
- update the relevant `project_app` rows;
- make non-secret environment values visible in the app configuration API;
- store secret values only through Vault/service secrets;
- verify marketplace asset metadata;
- validate seed jobs without executing them;
- create no deployment;
- contact no target server;
- call no Install Service operation;
- start no containers;
- expose no secret values in output or API responses.

The subsequent command:

```bash
stacker deploy
```

must generate a fresh runtime bundle from the synchronized project state and
upload it through the existing deployment flow.
