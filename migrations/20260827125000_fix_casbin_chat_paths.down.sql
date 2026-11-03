-- Revert chat Casbin rules back to /chat/* (without /api prefix).
DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN (
    '/api/chat/history',
    '/api/chat/sessions',
    '/api/chat/sessions/:id',
    '/api/chat/sessions/:id/messages',
    '/api/chat/sessions/:id/archive',
    '/api/chat/sessions/:id/unarchive'
  );

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
  ('p', 'group_user',  '/chat/history', 'GET',    '', '', ''),
  ('p', 'group_user',  '/chat/history', 'PUT',    '', '', ''),
  ('p', 'group_user',  '/chat/history', 'DELETE', '', '', ''),
  ('p', 'group_admin', '/chat/history', 'GET',    '', '', ''),
  ('p', 'group_admin', '/chat/history', 'PUT',    '', '', ''),
  ('p', 'group_admin', '/chat/history', 'DELETE', '', '', ''),
  ('p', 'group_user', '/chat/sessions',              'GET',    '', '', ''),
  ('p', 'group_user', '/chat/sessions',              'POST',   '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id',          'DELETE', '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id/messages', 'GET',    '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id/messages', 'POST',  '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id/messages', 'PUT',   '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id',          'PATCH', '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id/archive',   'POST', '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id/unarchive', 'POST', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
