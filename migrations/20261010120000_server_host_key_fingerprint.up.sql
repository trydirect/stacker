-- Trust-on-first-use pin for a server's SSH host key.
--
-- NULL means "nothing pinned yet": the next successful connection records
-- whatever the server presented. A non-NULL value is compared on every later
-- connection and a mismatch refuses the session instead of handing the far end
-- a shell (it would otherwise receive an authorized-key write or check commands).
--
-- The pin is cleared when srv_ip changes, because a new address means a new
-- machine and a re-provisioned server legitimately presents a new host key.
ALTER TABLE server ADD COLUMN IF NOT EXISTS host_key_fingerprint TEXT NULL;

COMMENT ON COLUMN server.host_key_fingerprint IS
    'Pinned SSH host key fingerprint in OpenSSH form (SHA256:base64). NULL = not pinned yet.';
