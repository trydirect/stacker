# Changelog

All notable changes to this project will be documented in this file.

## 2026-01-06

### Added
- Real HTTP-mocked tests for `UserServiceClient` covering user profile retrieval, product lookups, and template ownership checks.
- Integration-style webhook tests that verify the payloads emitted by `MarketplaceWebhookSender` for approved, updated, and rejected templates.
- Deployment validation tests ensuring plan gating and marketplace ownership logic behave correctly for free, paid, and plan-restricted templates.

