# Changelog

All notable changes to this project will be documented in this file.

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

