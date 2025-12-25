Important

- When implementing new endpoints, always add the Casbin rules (ACL).
- Recreate the database container to apply all database changes.

## Agent Registration Spec
- Endpoint: `POST /api/v1/agent/register`
- Body:
	- `deployment_hash: string` (required)
	- `capabilities: string[]` (optional)
	- `system_info: object` (optional)
	- `agent_version: string` (required)
	- `public_key: string | null` (optional; reserved for future use)
- Response:
	- `agent_id: string`
	- `agent_token: string` (also written to Vault)
	- `dashboard_version: string`
	- `supported_api_versions: string[]`

Notes:
- Token is stored in Vault at `{vault.agent_path_prefix}/{deployment_hash}/token`.
- If DB insert fails, the token entry is cleaned up.
- Add ACL rules for `POST /api/v1/agent/register`.