# Stacker — Development Roadmap

Forward-looking work items. Not yet scheduled unless noted.

---

## Destructive-operation protection (typed confirmation + optional 2FA)

**Status:** Proposed
**Priority:** High (security / data-loss prevention)

### Problem

Deletion of high-value resources (cloud tokens, servers, deployments, etc.) is
currently a single API call / CLI command with no guardrail. An accidental or
malicious call can destroy infrastructure or credentials irreversibly.

### Goal

Every destructive operation must require an explicit, deliberate confirmation
step before it executes. The strength of that step is **driven by
configuration**, so operators can tune friction vs. safety per environment.

### Requirements

1. **Typed confirmation (baseline).**
   Before deletion, the caller must re-enter the resource's own name/identifier
   manually (not a copy-paste of the id, ideally the human-readable name). The
   operation is rejected unless the typed value matches exactly.
   - CLI: interactive prompt (`Type the resource name to confirm: `).
   - API: require a `confirm_name` field in the request body that must equal
     the stored resource name.

2. **Configuration-driven policy.**
   A single config section (in `configuration.yaml`) controls behavior so it can
   differ between local / staging / production:

   ```yaml
   safety:
     destructive_ops:
       require_typed_confirmation: true      # global on/off
       require_2fa: true                     # gate below on 2FA when enabled
       # per-resource overrides
       resources:
         cloud_token:   { typed_confirmation: true, require_2fa: true }
         server:        { typed_confirmation: true, require_2fa: true }
         deployment:    { typed_confirmation: true, require_2fa: false }
         project:       { typed_confirmation: true, require_2fa: true }
   ```

   - Default to **safe** (confirmation on) when a resource is unlisted.
   - `--yes` / `--force` may bypass the *interactive prompt* only in
     non-production and only when explicitly allowed by config (never bypass
     2FA).

3. **2FA gating (when enabled).**
   For the most sensitive resources, if the user has 2FA enabled the operation
   must additionally require a fresh TOTP/second-factor challenge. If 2FA is
   configured as *required* for a resource but the user has none enrolled, the
   operation is refused with guidance to enroll.

4. **Audit trail.**
   Log who confirmed, when, what resource, and which factors were satisfied
   (typed-confirmation ✓ / 2FA ✓). Never log the resource's secret material.

### Resources that must be protected

Enumerated from current delete paths in `src/startup.rs` / `src/routes/**`:

| Resource | Route(s) | Typed confirm | 2FA (if enabled) |
|---|---|---|---|
| **Cloud token / credential** | `routes::cloud::delete` | ✅ | ✅ strongly |
| **Server** | `routes::server::delete` (+ `delete_preview`) | ✅ | ✅ |
| **Deployment** | deployment delete | ✅ | recommended |
| **Project** | `routes::project::delete` | ✅ | ✅ |
| **Project member (access revoke)** | `routes::project::member::delete` | ✅ | recommended |
| **Server / project secret** | `server::secret::delete`, `project::secret::delete` | ✅ | ✅ |
| **SSH key** | `server::ssh_key::delete_key` | ✅ | ✅ |
| **Pipe template / instance** | `pipe::delete_template/instance` | ✅ | recommended |

### Suggested additional resources warranting protection

Beyond deletion, these operations are destructive or security-sensitive enough
to deserve typed confirmation and/or 2FA:

- **Cloud/Vault token rotation or revocation** — losing a live token can orphan
  running infrastructure; require 2FA.
- **Server ownership / SSH key rotation** — changing the key that grants remote
  access; 2FA + typed confirmation.
- **Vault secret overwrite / bulk secret push** (`secrets push`) — silently
  replacing production secrets; require typed confirmation of the target env.
- **Firewall changes that close public ports / open `0.0.0.0`** — can take an app
  offline or expose it; confirm the port list.
- **Transfer of project/organization ownership** — irreversible authority change;
  2FA.
- **RBAC / Casbin policy changes** (`access_control.conf` via API) — privilege
  escalation risk; 2FA + audit.
- **Deploy/redeploy to production target** with `--force-rebuild` — data-losing
  restarts; typed confirmation of environment name.
- **Draining/destroying a cloud VM or downsizing** — same class as server delete.
- **Marketplace unpublish / template deletion** — affects downstream consumers.
- **Account-level actions**: disabling 2FA, deleting the user account, revoking
  all sessions/tokens — each should itself require the *current* second factor.

### Implementation notes

- Introduce a small `ConfirmationPolicy` helper resolved from config, checked in
  a shared middleware or a guard invoked by each destructive handler — avoid
  scattering ad-hoc checks.
- Keep the typed-confirmation contract identical across API and CLI so the CLI is
  a thin prompt over the same API field.
- Add validation code(s) and refusal messages consistent with existing E/W
  codes.

### Open questions

- Where is 2FA enrollment state stored today, and is there an existing TOTP
  verifier to reuse?
- Should confirmation be a short-lived signed "confirmation token" (two-step:
  request → confirm) to avoid TOCTOU between listing and deleting?
