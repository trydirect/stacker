-- Allow admins to edit a template's content: PUT /admin/templates/:id
-- Mirrors the existing per-field admin rules (pricing, verifications) across
-- both gateway prefixes and all three admin roles.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'admin_service', '/api/admin/templates/:id', 'PUT', '', '', '')
ON CONFLICT DO NOTHING;

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'group_admin', '/api/admin/templates/:id', 'PUT', '', '', '')
ON CONFLICT DO NOTHING;

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'root', '/api/admin/templates/:id', 'PUT', '', '', '')
ON CONFLICT DO NOTHING;

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'admin_service', '/stacker/admin/templates/:id', 'PUT', '', '', '')
ON CONFLICT DO NOTHING;

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'group_admin', '/stacker/admin/templates/:id', 'PUT', '', '', '')
ON CONFLICT DO NOTHING;

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'root', '/stacker/admin/templates/:id', 'PUT', '', '', '')
ON CONFLICT DO NOTHING;
