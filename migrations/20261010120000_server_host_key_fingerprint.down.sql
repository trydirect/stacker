-- Revert the trust-on-first-use host key pin.
ALTER TABLE server DROP COLUMN IF EXISTS host_key_fingerprint;
