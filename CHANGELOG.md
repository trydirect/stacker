# Changelog

All notable changes to this project will be documented in this file.

## 2026-01-29

### Added - Unified Configuration Management System

#### ConfigRenderer Service (`src/services/config_renderer.rs`)
- New `ConfigRenderer` service that converts `ProjectApp` records to deployable configuration files
- Tera template engine integration for rendering docker-compose.yml and .env files
- Embedded templates: `docker-compose.yml.tera`, `env.tera`, `service.tera`
- Support for multiple input formats: JSON object, JSON array, string (docker-compose style)
- Automatic Vault sync via `sync_to_vault()` and `sync_app_to_vault()` methods

#### ProjectAppService (`src/services/project_app_service.rs`)
- High-level service wrapping database operations with automatic Vault sync
- Create/Update/Delete operations trigger config rendering and Vault storage
- `sync_all_to_vault()` for bulk deployment sync
- `preview_bundle()` for config preview without syncing
- Validation for app code format, required fields

#### Config Versioning (`project_app` table)
- New columns: `config_version`, `vault_synced_at`, `vault_sync_version`, `config_hash`
- `needs_vault_sync()` method to detect out-of-sync configs
- `increment_version()` and `mark_synced()` helper methods
- Migration: `20260129120000_add_config_versioning`

#### Dependencies
- Added `tera = "1.19.1"` for template rendering

## 2026-01-26

### Fixed - Deployment Hash Not Sent to Install Service

#### Bug Fix: `saved_item()` endpoint missing `deployment_hash` in RabbitMQ payload
- **Issue**: The `POST /{id}/deploy/{cloud_id}` endpoint (for deployments with saved cloud credentials) was generating a `deployment_hash` and saving it to the database, but NOT including it in the RabbitMQ message payload sent to the install service.
- **Root Cause**: In `src/routes/project/deploy.rs`, the `saved_item()` function published the payload without setting `payload.deployment_hash`, unlike the `item()` function which correctly delegates to `InstallServiceClient.deploy()`.
- **Fix**: Added `payload.deployment_hash = Some(deployment_hash.clone())` before publishing to RabbitMQ.
- **Files Changed**: `src/routes/project/deploy.rs`

## 2026-01-24

### Added - App Configuration Editor (Backend)

#### Project App Model & Database (`project_app`)
- New `ProjectApp` model with fields: environment (JSONB), ports (JSONB), volumes, domain, ssl_enabled, resources, restart_policy, command, entrypoint, networks, depends_on, healthcheck, labels, enabled, deploy_order
- Database CRUD operations in `src/db/project_app.rs`: fetch, insert, update, delete, fetch_by_project_and_code
- Migration `20260122120000_create_project_app_table` with indexes and triggers

#### REST API Routes (`/project/{id}/apps/*`)
- `GET /project/{id}/apps` - List all apps for a project
- `GET /project/{id}/apps/{code}` - Get single app details
- `GET /project/{id}/apps/{code}/config` - Get full app configuration
- `GET /project/{id}/apps/{code}/env` - Get environment variables (sensitive values redacted)
- `PUT /project/{id}/apps/{code}/env` - Update environment variables
- `PUT /project/{id}/apps/{code}/ports` - Update port mappings
- `PUT /project/{id}/apps/{code}/domain` - Update domain/SSL settings

#### Support Documentation
- Added `docs/SUPPORT_ESCALATION_GUIDE.md` - AI support escalation handling for support team

### Fixed - MCP Tools Type Errors
- Fixed type comparison errors in `compose.rs` and `config.rs`:
  - `project.user_id` is `String` (not `Option<String>`) - use direct comparison
  - `deployment.user_id` is `Option<String>` - use `as_deref()` for comparison
  - `app.code` and `app.image` are `String` (not `Option<String>`)
  - Replaced non-existent `cpu_limit`/`memory_limit` fields with `resources` JSONB

## 2026-01-23

### Added - Vault Configuration Management

#### Vault Configuration Tools (Phase 5 continuation)
- `get_vault_config`: Fetch app configuration from HashiCorp Vault by deployment hash and app code
- `set_vault_config`: Store app configuration in Vault (content, content_type, destination_path, file_mode)
- `list_vault_configs`: List all app configurations stored in Vault for a deployment
- `apply_vault_config`: Queue apply_config command to Status Panel agent for config deployment

#### VaultService (`src/services/vault_service.rs`)
- New service for Vault KV v2 API integration
- Path template: `{prefix}/{deployment_hash}/apps/{app_name}/config`
- Methods: `fetch_app_config()`, `store_app_config()`, `list_app_configs()`, `delete_app_config()`
- Environment config: `VAULT_ADDRESS`, `VAULT_TOKEN`, `VAULT_AGENT_PATH_PREFIX`

### Changed
- Updated `src/services/mod.rs` to export `VaultService`, `AppConfig`, `VaultError`
- Updated `src/mcp/registry.rs` to register 4 new Vault config tools (total: 41 tools)

## 2026-01-22

### Added - Phase 5: Agent-Based App Deployment & Configuration Management

#### Container Operations Tools
- `stop_container`: Gracefully stop a specific container in a deployment with configurable timeout
- `start_container`: Start a previously stopped container
- `get_error_summary`: Analyze container logs and return categorized error counts, patterns, and suggestions

#### App Configuration Management Tools (new `config.rs` module)
- `get_app_env_vars`: View environment variables for an app (with automatic redaction of sensitive values)
- `set_app_env_var`: Create or update an environment variable
- `delete_app_env_var`: Remove an environment variable
- `get_app_config`: Get full app configuration including ports, volumes, domain, SSL, and resource limits
- `update_app_ports`: Configure port mappings for an app
- `update_app_domain`: Set domain and SSL configuration for web apps

#### Stack Validation Tool
- `validate_stack_config`: Pre-deployment validation checking for missing images, port conflicts, database passwords, and common misconfigurations

#### Integration Testing & Documentation
- Added `stacker/tests/mcp_integration.rs`: Comprehensive User Service integration tests
- Added `stacker/docs/SLACK_WEBHOOK_SETUP.md`: Production Slack webhook configuration guide
- Added new environment variables to `env.dist`: `SLACK_SUPPORT_WEBHOOK_URL`, `TAWK_TO_*`, `USER_SERVICE_URL`

### Changed
- Updated `stacker/src/mcp/tools/mod.rs` to export new `config` module
- Updated `stacker/src/mcp/registry.rs` to register 10 new MCP tools (total: 37 tools)
- Updated AI-INTEGRATION-PLAN.md with Phase 5 implementation status and test documentation

## 2026-01-06

### Added
- Real HTTP-mocked tests for `UserServiceClient` covering user profile retrieval, product lookups, and template ownership checks.
- Integration-style webhook tests that verify the payloads emitted by `MarketplaceWebhookSender` for approved, updated, and rejected templates.
- Deployment validation tests ensuring plan gating and marketplace ownership logic behave correctly for free, paid, and plan-restricted templates.

## 2026-01-16

### Added
- Configurable agent command polling defaults via config and environment variables.
- Configurable Casbin reload enablement and interval.

### Changed
- OAuth token validation uses a shared HTTP client and short-lived cache for reduced latency.
- Agent command polling endpoint accepts optional `timeout` and `interval` parameters.
- Casbin reload is guarded to avoid blocking request handling and re-applies route matching after reload.

### Fixed
- Status panel command updates query uses explicit bindings to avoid SQLx type inference errors.

