-- The project owner's saved clouds (no credentials), for the owner and members.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/project/:id/clouds', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/project/:id/clouds', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/v1/project/:id/clouds', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/v1/project/:id/clouds', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
