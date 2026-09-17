-- `POST /api/v1/agent/rotate-token/{deployment_hash}` reissues an agent's
-- bearer token. Ownership is checked in the handler by
-- `authorize_deployment_access`, so the policy only has to admit the roles that
-- may act on their own deployments.
--
-- Deliberately not granted to `group_anonymous`: unlike registration, the
-- caller here always has a credential.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_user', '/api/v1/agent/rotate-token/*', 'POST', '', '', ''),
    ('p', 'group_admin', '/api/v1/agent/rotate-token/*', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/agent/rotate-token/*', 'POST', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
