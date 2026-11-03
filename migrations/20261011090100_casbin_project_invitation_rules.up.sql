-- Team seats: invitations, seat counts and adding members.
-- The invitation lookup by token is public so the invitation page works
-- before the invitee has an account; it reveals only a masked address.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/project/:id/members', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/project/:id/invitations', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/project/:id/invitations', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/project/:id/invitations/:invitation_id', 'DELETE', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/project/:id/seats', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/project/invitations/pending', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/project/invitations/:key/accept', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_anonymous', '/project/invitations/:token', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/project/:id/members', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/project/:id/invitations', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/project/:id/invitations', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/project/:id/invitations/:invitation_id', 'DELETE', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/project/:id/seats', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/project/invitations/pending', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/project/invitations/:key/accept', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/v1/project/:id/members', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/v1/project/:id/invitations', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/v1/project/:id/invitations', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/v1/project/:id/invitations/:invitation_id', 'DELETE', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/v1/project/:id/seats', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/v1/project/invitations/pending', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/v1/project/invitations/:key/accept', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_anonymous', '/api/v1/project/invitations/:token', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/v1/project/:id/members', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/v1/project/:id/invitations', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/v1/project/:id/invitations', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/v1/project/:id/invitations/:invitation_id', 'DELETE', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/v1/project/:id/seats', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/v1/project/invitations/pending', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/v1/project/invitations/:key/accept', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
