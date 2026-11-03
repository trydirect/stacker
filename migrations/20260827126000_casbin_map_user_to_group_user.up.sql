-- Map the "user" role (returned by the auth service) to "group_user"
-- so that regular users inherit all group_user Casbin permissions.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('g', 'user', 'group_user', '', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
