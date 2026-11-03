-- Casbin rules for the admin-ui notification bell "mark all read"
-- (src/routes/admin_notifications.rs). Same dual-prefix, multi-role
-- convention as 20261005090000_casbin_admin_notifications_rules.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_admin',   '/api/admin/notifications/mark-all-read',         'POST', '', '', ''),
    ('p', 'admin_service', '/api/admin/notifications/mark-all-read',         'POST', '', '', ''),
    ('p', 'root',          '/api/admin/notifications/mark-all-read',         'POST', '', '', ''),
    ('p', 'group_admin',   '/stacker/api/admin/notifications/mark-all-read', 'POST', '', '', ''),
    ('p', 'admin_service', '/stacker/api/admin/notifications/mark-all-read', 'POST', '', '', ''),
    ('p', 'root',          '/stacker/api/admin/notifications/mark-all-read', 'POST', '', '', '')
ON CONFLICT DO NOTHING;
