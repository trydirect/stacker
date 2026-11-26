-- Add Casbin rules for /api/v1/backups endpoints (WS-1 1.5)
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_user', '/api/v1/backups/destinations', 'GET', '', '', ''),
    ('p', 'group_user', '/api/v1/backups/destinations', 'POST', '', '', ''),
    ('p', 'group_user', '/api/v1/backups/destinations/:id', 'DELETE', '', '', ''),
    ('p', 'group_user', '/api/v1/backups/jobs', 'GET', '', '', ''),
    ('p', 'group_user', '/api/v1/backups/jobs', 'POST', '', '', ''),
    ('p', 'group_user', '/api/v1/backups/jobs/:id', 'GET', '', '', ''),
    ('p', 'group_user', '/api/v1/backups/jobs/:id', 'PATCH', '', '', ''),
    ('p', 'group_user', '/api/v1/backups/jobs/:id', 'DELETE', '', '', ''),
    ('p', 'group_user', '/api/v1/backups/jobs/:id/runs', 'GET', '', '', ''),
    ('p', 'group_user', '/api/v1/backups/jobs/:id/run', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/backups/destinations', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/backups/destinations', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/backups/destinations/:id', 'DELETE', '', '', ''),
    ('p', 'root', '/api/v1/backups/jobs', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/backups/jobs', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/backups/jobs/:id', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/backups/jobs/:id', 'PATCH', '', '', ''),
    ('p', 'root', '/api/v1/backups/jobs/:id', 'DELETE', '', '', ''),
    ('p', 'root', '/api/v1/backups/jobs/:id/runs', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/backups/jobs/:id/run', 'POST', '', '', '');
