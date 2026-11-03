-- Casbin ACL rules for the 1-Click Deploy endpoints.
-- /api/v1/deploy/validate is public (anonymous) — it parses/validates arbitrary
-- stacker.yml before the user reaches server configuration.
-- /api/v1/deploy/clone requires an authenticated user (deploys onto TryDirect Hetzner).

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES
  -- validate: public, all roles
  ('p', 'group_anonymous', '/api/v1/deploy/validate', 'POST', '', '', ''),
  ('p', 'group_user', '/api/v1/deploy/validate', 'POST', '', '', ''),
  ('p', 'group_admin', '/api/v1/deploy/validate', 'POST', '', '', ''),
  ('p', 'agent', '/api/v1/deploy/validate', 'POST', '', '', ''),
  -- clone: authenticated users + admin only
  ('p', 'group_user', '/api/v1/deploy/clone', 'POST', '', '', ''),
  ('p', 'group_admin', '/api/v1/deploy/clone', 'POST', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
