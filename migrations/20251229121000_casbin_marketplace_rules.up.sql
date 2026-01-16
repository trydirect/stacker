-- Casbin rules for Marketplace endpoints

-- Public read rules
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_anonymous', '/api/templates', 'GET', '', '', '');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_anonymous', '/api/templates/:slug', 'GET', '', '', '');

-- Creator rules
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/templates', 'POST', '', '', '');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/templates/:id', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/templates/:id/submit', 'POST', '', '', '');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_user', '/api/templates/mine', 'GET', '', '', '');

-- Admin moderation rules
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/admin/templates', 'GET', '', '', '');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/admin/templates/:id/approve', 'POST', '', '', '');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/admin/templates/:id/reject', 'POST', '', '', '');
