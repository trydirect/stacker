INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_admin',    '/api/admin/vendors/:creator_user_id/vendor-profile', 'PATCH', '', '', ''),
    ('p', 'admin_service',  '/api/admin/vendors/:creator_user_id/vendor-profile', 'PATCH', '', '', ''),
    ('p', 'group_admin',    '/stacker/api/admin/vendors/:creator_user_id/vendor-profile', 'PATCH', '', '', ''),
    ('p', 'admin_service',  '/stacker/api/admin/vendors/:creator_user_id/vendor-profile', 'PATCH', '', '', '')
ON CONFLICT DO NOTHING;
