-- Add Casbin rules for /api/v1/sandboxes endpoints (WS-S)
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_user', '/api/v1/sandboxes', 'GET', '', '', ''),
    ('p', 'group_user', '/api/v1/sandboxes', 'POST', '', '', ''),
    ('p', 'group_user', '/api/v1/sandboxes/:id', 'GET', '', '', ''),
    ('p', 'group_user', '/api/v1/sandboxes/:id', 'DELETE', '', '', ''),
    ('p', 'group_user', '/api/v1/sandboxes/:id/exec', 'POST', '', '', ''),
    ('p', 'group_user', '/api/v1/sandboxes/:id/files', 'GET', '', '', ''),
    ('p', 'group_user', '/api/v1/sandboxes/:id/files', 'POST', '', '', ''),
    ('p', 'group_user', '/api/v1/sandboxes/:id/files/list', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/sandboxes', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/sandboxes', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/sandboxes/:id', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/sandboxes/:id', 'DELETE', '', '', ''),
    ('p', 'root', '/api/v1/sandboxes/:id/exec', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/sandboxes/:id/files', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/sandboxes/:id/files', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/sandboxes/:id/files/list', 'GET', '', '', '');
