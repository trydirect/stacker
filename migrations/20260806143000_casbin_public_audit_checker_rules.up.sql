-- Public audit checkers exposed via /api/audit/* (compose, dockerfile, exposure,
-- readiness, cost, image). Fronted by blog Next.js proxy /api/audit/[checker].js
-- Anonymous access — these are marketing/tools endpoints.

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_anonymous', '/api/audit/compose',    'POST', '', '', ''),
    ('p', 'group_anonymous', '/api/audit/dockerfile', 'POST', '', '', ''),
    ('p', 'group_anonymous', '/api/audit/exposure',   'POST', '', '', ''),
    ('p', 'group_anonymous', '/api/audit/readiness',  'POST', '', '', ''),
    ('p', 'group_anonymous', '/api/audit/cost',       'POST', '', '', ''),
    ('p', 'group_anonymous', '/api/audit/image',      'POST', '', '', ''),
    ('p', 'group_user',      '/api/audit/compose',    'POST', '', '', ''),
    ('p', 'group_user',      '/api/audit/dockerfile', 'POST', '', '', ''),
    ('p', 'group_user',      '/api/audit/exposure',   'POST', '', '', ''),
    ('p', 'group_user',      '/api/audit/readiness',  'POST', '', '', ''),
    ('p', 'group_user',      '/api/audit/cost',       'POST', '', '', ''),
    ('p', 'group_user',      '/api/audit/image',      'POST', '', '', ''),
    ('p', 'group_admin',     '/api/audit/compose',    'POST', '', '', ''),
    ('p', 'group_admin',     '/api/audit/dockerfile', 'POST', '', '', ''),
    ('p', 'group_admin',     '/api/audit/exposure',   'POST', '', '', ''),
    ('p', 'group_admin',     '/api/audit/readiness',  'POST', '', '', ''),
    ('p', 'group_admin',     '/api/audit/cost',       'POST', '', '', ''),
    ('p', 'group_admin',     '/api/audit/image',      'POST', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
