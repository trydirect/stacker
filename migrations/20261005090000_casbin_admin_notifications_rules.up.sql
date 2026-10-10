-- Casbin rules for the admin-ui notification bell proxy
-- (src/routes/admin_notifications.rs). Mirrors the dual-prefix,
-- multi-role convention already used for /api/admin/vendors and
-- /api/admin/templates/:id/unapprove.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_admin',   '/api/admin/notifications',                 'GET',   '', '', ''),
    ('p', 'admin_service', '/api/admin/notifications',                 'GET',   '', '', ''),
    ('p', 'root',          '/api/admin/notifications',                 'GET',   '', '', ''),
    ('p', 'group_admin',   '/stacker/api/admin/notifications',         'GET',   '', '', ''),
    ('p', 'admin_service', '/stacker/api/admin/notifications',         'GET',   '', '', ''),
    ('p', 'root',          '/stacker/api/admin/notifications',         'GET',   '', '', ''),

    ('p', 'group_admin',   '/api/admin/notifications/unread-count',         'GET', '', '', ''),
    ('p', 'admin_service', '/api/admin/notifications/unread-count',         'GET', '', '', ''),
    ('p', 'root',          '/api/admin/notifications/unread-count',         'GET', '', '', ''),
    ('p', 'group_admin',   '/stacker/api/admin/notifications/unread-count', 'GET', '', '', ''),
    ('p', 'admin_service', '/stacker/api/admin/notifications/unread-count', 'GET', '', '', ''),
    ('p', 'root',          '/stacker/api/admin/notifications/unread-count', 'GET', '', '', ''),

    ('p', 'group_admin',   '/api/admin/notifications/:id',                 'PATCH', '', '', ''),
    ('p', 'admin_service', '/api/admin/notifications/:id',                 'PATCH', '', '', ''),
    ('p', 'root',          '/api/admin/notifications/:id',                 'PATCH', '', '', ''),
    ('p', 'group_admin',   '/stacker/api/admin/notifications/:id',         'PATCH', '', '', ''),
    ('p', 'admin_service', '/stacker/api/admin/notifications/:id',         'PATCH', '', '', ''),
    ('p', 'root',          '/stacker/api/admin/notifications/:id',         'PATCH', '', '', '')
ON CONFLICT DO NOTHING;
