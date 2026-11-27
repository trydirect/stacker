# Runtime env and secrets delivery

How environment variables and secrets get from `stacker.yml`, the project env
file and Vault into the files Docker Compose reads on the server — on both deploy
paths. Read this before touching `.env` handling, secrets, or compose paths, and
before debugging `required variable X is missing a value`.

Related: [APP_DEPLOYMENT.md](APP_DEPLOYMENT.md) (Vault layout, agent tokens),
[SECRET_LIFECYCLE.md](SECRET_LIFECYCLE.md) (a secret's path through a marketplace
bake).

## Two files, two jobs

| File | Read by | Job |
|---|---|---|
| `<stack dir>/.env` — next to the compose the agent runs | Docker Compose | **Interpolation** of `${VAR}`, `${VAR:?…}`, `${VAR:-…}` in the compose. Compose reads `${VAR}` only from here (or `--env-file`). |
| per-service `env_file:` | the container | **Container environment**. Never feeds `${VAR}` interpolation. |

A secret used as `${SECRET:?…}` in the compose must be in `<stack dir>/.env`. A
secret only the app reads at runtime belongs in that service's `env_file`.

## Where values come from

Three layers, lowest precedence first:

1. **Authored app env** — `services[].environment` in `stacker.yml`, stored as
   `project_app.environment`. Often placeholders (a dummy database password).
2. **Project env file** — `environments.<env>.env_file` (e.g. `docker/prod/.env`).
   The CLI ships it as the config file with `destination_path: ".env"`
   (`src/cli/config_bundle.rs`, `build_config_bundle`).
3. **Vault service secrets** — `stacker secrets set KEY --scope service --service <app>`;
   metadata in `remote_secret` (`scope = 'service'`), values in Vault
   (`ConfigRenderer::load_service_secrets`).

Reserved prefixes `STACKER_`, `DOCKER_`, `VAULT_`, `AGENT_` are rejected in
authored env, allowed in service secrets, **dropped** from the project union and
**kept** in an app's `env_file` merge.

A value that exists in none of these layers cannot be delivered by any code
path. Check first: `stacker secrets list --scope service --service <app>` for
every target (prints names only).

## Path 1 — agent `deploy_app` (`stacker secrets push`, `stacker agent deploy-app`)

1. **CLI** (`src/console/commands/cli/agent.rs`,
   `collect_local_config_files_for_agent_deploy`):
   - app without its own compose → bundles the **project compose** from the
     project root and ships the project env file as `.env`;
   - app with `<app>/docker/<env>/compose.yml` → bundles that compose from its own
     directory and merges its service into the project compose text; the project
     `.env` is **not** shipped.
2. **Stacker server** (`src/routes/command/create.rs`, `enrich_deploy_app_with_compose`):
   - renders the target app's env (`render_app_env_config`) and merges it into the
     `env_file`s that app's compose references (`merge_rendered_env_into_app_env_files`);
   - renders the **project union** (`ConfigRenderer::resolve_project_runtime_env`):
     authored env of all enabled apps < project env file < service secrets of all
     enabled apps. The project env file is the `.env` the command carries, else the
     copy in the deployment record's `metadata.config_files`
     (`project_env_file_for_union`);
   - puts the union into `params.env_vars` (`insert_project_runtime_env_vars`),
     unless the caller sent its own non-empty `env_vars` or the union is empty;
   - once the union is delivered, drops the single-app file it emitted to
     `remote_runtime_env_path_for(stack_code)` (`drop_superseded_runtime_env_file`,
     #400). That path is only Stacker's guess of the stack dir and is shared by
     every app.
3. **Status agent** (`status` repo, `src/commands/stacker.rs`, `handle_deploy_app`):
   - finds the compose dir from what is on disk (`resolve_compose_paths_for_service`);
   - writes `config_files` (drift guard below), then writes `env_vars` to
     `<compose dir>/.env` — **replacing the whole file**, after a
     `.env.stacker-bak-<ts>` backup;
   - runs `docker compose up -d <service>` (only the target service).

### Drift guard

Rendered env files start with `# stacker-render version=… hash=… inputs=…`. Before
overwriting, the agent compares the new file's header hash with a hash of the file
on disk; on mismatch it refuses unless the command is forced (`--force`).

Both sides must hash the same way: drop comment/blank lines and Stacker-owned keys
(`DEPLOYMENT_HASH`, reserved prefixes), sort the rest, sha256 of `key=value\n`.
Stacker: `hash_env_body_semantic` (`src/services/config_renderer.rs`); agent:
`env_body_hash` (status PR #153). When they disagree, every second write reports
drift.

## Path 2 — full `stacker deploy` (install service + Ansible)

1. **CLI** uploads `config_files`, including the project env file as `.env`
   (`build_config_bundle`).
2. **Stacker server** (`src/routes/project/deploy.rs`):
   - `apply_deploy_bundle` copies `config_files` into `project.metadata`, but the
     deploy route **never saves the project row** (`db::project::update` runs only in
     `rollback`). The copy that persists is `deployment.metadata`.
   - `render_full_deploy_env_file` layers the project's Vault service secrets over
     the `.env` (`layer_secrets_into_env_file`: overridden keys replaced, missing
     ones appended). It runs before the deployment record is inserted, so a Vault
     failure stops the deploy without a pending record. Authored app env is **not**
     added on this path.
   - `dispatch_project_with_env_file` puts the result, mode `0600`, only into a
     **copy** of the project for `install_service.deploy` (`config_files` and
     `custom.marketplace_config_files`). The deployment record keeps the unmodified
     metadata. Without service secrets the payload is unchanged.
3. **Install service** stages `config_files` into the remote project dir
   (`install` repo, `app/Ansible.py`, `collect_stageable_config_files`;
   `DEFAULT_REMOTE_PROJECT_DIR = "/home/trydirect/project"`), then Ansible runs
   `docker compose up`.

## Compose paths

- **Project compose paths are project-root-relative** — the remote compose runs
  from the project root. The CLI resolves them from the root on both paths (#398).
- `../` escapes are rejected (`env_file_outside_project`). A path that exists only
  relative to the compose file, or that resolves to a different file from each base
  (e.g. a dev `.env` at the root), is an error naming the root-relative path to
  write (`compose_relative_conflict`). Bind-mount suggestions keep `./` (a bare
  relative source reads as a named volume). Directory mounts are never uploaded.
- An app-local `<app>/docker/<env>/compose.yml` keeps its own directory as base.
- The status agent also refuses an `env_file` that resolves outside the compose
  project directory.

Run a compose locally the way the server does:
`docker compose --project-directory <repo root> -f docker/prod/compose.yml config`.

## When `${VAR:?}` fails — checklist

1. Is the value stored? `stacker secrets list --scope service --service <app>` for
   each app. A secret that is only in a file on the server is not "in Vault".
2. Which path ran? Full deploy: deployment `paused`, Ansible log with `Copy files`
   and `docker compose … up`. Agent path: a `deploy_app` command. The two paths
   build `.env` differently (above).
3. On the server, list **key names only** of `<stack dir>/.env` and run
   `docker compose -f <stack dir>/docker-compose.yml config >/dev/null`.
4. Drift error: compare the file's header hash with its semantic hash. Equal → the
   file belongs to another render (another app wrote it); since #400 this should no
   longer happen for the single-app file, `--force` gets past it on older builds.
   Different → the file was edited after Stacker wrote it, or the agent predates
   status #153.
5. Do not hand-edit files on the server; store the value with `stacker secrets set`
   and push.

## Known gaps

- `stack_code` is recomputed as `<name>-<id>` (`render_app_env_config`,
  `deployment_state`, `explain`) instead of using the stack dir the deployment was
  sent with; projects on the legacy `/home/trydirect/project` layout get wrong
  paths in those places.
- `/opt/stacker/deployments/<env>/files/` is documented in APP_DEPLOYMENT.md for
  app-local bundles, but no current writer was found in the CLI or server code;
  servers deployed earlier still have files there. It is keyed by environment only,
  so two projects with the same env name would share it.
- Values are written unquoted; Compose may interpolate a `$` inside a value.
- `env_vars` (with secrets) are stored in `commands.parameters`; the read APIs
  redact them.
