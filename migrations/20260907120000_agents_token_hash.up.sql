-- Store a hash of the agent bearer token so authentication no longer has to
-- read the plaintext secret out of Vault. See docs and the auth middleware in
-- src/middleware/authentication/method/f_agent.rs.
ALTER TABLE agents ADD COLUMN IF NOT EXISTS token_hash TEXT;
-- TIMESTAMPTZ, not TIMESTAMP: the original CREATE TABLE used naive timestamps,
-- but 20251222225538_timestamptz_for_agents_deployments_commands converted this table's columns to timestamptz. `Agent` maps
-- them to `DateTime<Utc>`, which cannot decode a naive column — and because
-- `query_as` resolves types at runtime, a mismatch here fails every agent read
-- with a bare "Database error", not a compile error.
ALTER TABLE agents ADD COLUMN IF NOT EXISTS token_hash_updated_at TIMESTAMPTZ;

COMMENT ON COLUMN agents.token_hash IS
  'Algorithm-tagged digest of the agent bearer token, e.g. sha256:<64 hex>. Never the token itself.';
