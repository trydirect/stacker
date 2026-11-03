-- Record which ${VAR} references the baked compose actually needs, so the clone
-- path can refuse a deploy whose env file would not satisfy them.
--
-- Without this, a missing key is silent: Docker Compose substitutes an empty
-- string with only a warning, the unit's ExecStartPre ends in `|| true`, and the
-- systemd unit still reports active while the stack is misconfigured.
--
-- Nullable on purpose: snapshots baked before this column carry NULL and skip
-- the check, so existing images keep deploying unchanged.
ALTER TABLE baked_snapshots ADD COLUMN required_env_keys JSONB;
