INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (1, 'g', 'anonym', 'group_anonymous', '', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (2, 'g', 'group_admin', 'group_anonymous', '', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (3, 'g', 'group_user', 'group_anonymous', '', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (4, 'g', 'user', 'group_user', '', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (5, 'g', 'admin_petru', 'group_admin', '', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (6, 'g', 'user_petru', 'group_user', '', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (7, 'p', 'group_anonymous', '/health_check', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (8, 'p', 'group_anonymous', '/rating/:id', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (9, 'p', 'group_anonymous', '/rating', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (10, 'p', 'group_admin', '/client', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (11, 'p', 'group_admin', '/rating', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (12, 'p', 'group_admin', '/admin/client/:id/disable', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (13, 'p', 'group_admin', '/admin/client/:id/enable', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (14, 'p', 'group_admin', '/admin/client/:id', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (15, 'p', 'group_admin', '/admin/project/user/:userid', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (16, 'p', 'group_admin', '/rating/:id', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (17, 'p', 'group_user', '/client/:id/enable', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (18, 'p', 'group_user', '/client/:id', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (19, 'p', 'group_user', '/client/:id/disable', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (20, 'p', 'group_user', '/rating/:id', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (21, 'p', 'group_user', '/rating', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (22, 'p', 'group_user', '/rating', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (23, 'p', 'group_user', '/project', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (24, 'p', 'group_user', '/project', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (25, 'p', 'group_user', '/project/:id', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (26, 'p', 'group_user', '/project/:id', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (27, 'p', 'group_user', '/project/:id', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (28, 'p', 'group_user', '/project/:id', 'DELETE', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (29, 'p', 'group_user', '/project/:id/compose', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (30, 'p', 'group_user', '/project/:id/compose', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (31, 'p', 'group_user', '/project/:id/deploy', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (32, 'p', 'group_user', '/project/:id/deploy/:cloud_id', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (33, 'p', 'group_user', '/server', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (34, 'p', 'group_user', '/server', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (35, 'p', 'group_user', '/server/:id', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (36, 'p', 'group_user', '/server/:id', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (37, 'p', 'group_user', '/cloud', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (38, 'p', 'group_user', '/cloud', 'POST', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (39, 'p', 'group_user', '/cloud/:id', 'GET', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (40, 'p', 'group_user', '/cloud/:id', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (id, ptype, v0, v1, v2, v3, v4, v5) VALUES (41, 'p', 'group_user', '/cloud/:id', 'DELETE', '', '', '');
