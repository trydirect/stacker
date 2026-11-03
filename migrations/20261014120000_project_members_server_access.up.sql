-- Team seats: the owner decides whether members get the servers' SSH key and
-- passwords (the deployment report). Off by default.
ALTER TABLE project ADD COLUMN IF NOT EXISTS members_server_access BOOLEAN NOT NULL DEFAULT FALSE;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/project/:id/server-access', 'PATCH', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/v1/project/:id/server-access', 'PATCH', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/project/:id/server-access', 'PATCH', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/v1/project/:id/server-access', 'PATCH', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
