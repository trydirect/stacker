INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_user', '/api/templates/:slug/install', 'POST', '', '', ''),
    ('p', 'group_user', '/api/v1/templates/:slug/install', 'POST', '', '', '')
ON CONFLICT DO NOTHING;
