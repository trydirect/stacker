-- Add Casbin rules for the GitHub Repo Connect endpoints (WS-G G4).

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_user', '/api/v1/github/repos', 'GET', '', '', ''),
    ('p', 'group_user', '/api/v1/github/repos', 'POST', '', '', ''),
    ('p', 'group_user', '/api/v1/github/repos/:id', 'PATCH', '', '', ''),
    ('p', 'group_user', '/api/v1/github/repos/:id', 'DELETE', '', '', ''),
    ('p', 'root', '/api/v1/github/repos', 'GET', '', '', ''),
    ('p', 'root', '/api/v1/github/repos', 'POST', '', '', ''),
    ('p', 'root', '/api/v1/github/repos/:id', 'PATCH', '', '', ''),
    ('p', 'root', '/api/v1/github/repos/:id', 'DELETE', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
