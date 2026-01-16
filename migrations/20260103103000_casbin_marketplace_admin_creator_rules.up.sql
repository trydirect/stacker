-- Allow admin service accounts (e.g., root) to call marketplace creator endpoints
-- Admins previously lacked creator privileges which caused 403 responses
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/templates', 'POST', '', '', '');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/templates/:id', 'PUT', '', '', '');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/templates/:id/submit', 'POST', '', '', '');
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/templates/mine', 'GET', '', '', '');
