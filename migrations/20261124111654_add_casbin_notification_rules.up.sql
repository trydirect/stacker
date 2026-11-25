-- Add Casbin rules for /api/v1/notifications endpoints (WS-N)

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_user', '/api/v1/notifications/channels', 'GET', '', '', ''),
    ('p', 'group_user', '/api/v1/notifications/channels', 'POST', '', '', ''),
    ('p', 'group_user', '/api/v1/notifications/channels/:id', 'PATCH', '', '', ''),
    ('p', 'group_user', '/api/v1/notifications/channels/:id', 'DELETE', '', '', ''),
    ('p', 'group_user', '/api/v1/notifications/channels/:id/test', 'POST', '', '', ''),
    ('p', 'group_user', '/api/v1/notifications/dispatches', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/notifications/channels', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/notifications/channels', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/notifications/channels/:id', 'PATCH', '', '', ''),
    ('p', 'root', '/api/v1/notifications/channels/:id', 'DELETE', '', '', ''),
    ('p', 'root', '/api/v1/notifications/channels/:id/test', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/notifications/dispatches', 'GET', '', '', '')
ON CONFLICT DO NOTHING;
