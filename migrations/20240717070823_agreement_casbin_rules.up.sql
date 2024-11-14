-- Add up migration script here

INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (49, 'p', 'group_user', '/agreement', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (50, 'p', 'group_user', '/agreement/:id', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (51, 'p', 'group_admin', '/agreement', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (52, 'p', 'group_admin', '/agreement/:id', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (53, 'p', 'group_admin', '/admin/agreement', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (54, 'p', 'group_admin', '/admin/agreement/:id', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (55, 'p', 'group_admin', '/admin/agreement/:id', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (56, 'p', 'group_admin', '/admin/agreement/:id', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (57, 'p', 'group_admin', '/admin/agreement/:id', 'DELETE', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (58, 'p', 'group_user', '/agreement', 'POST', '', '', '');
