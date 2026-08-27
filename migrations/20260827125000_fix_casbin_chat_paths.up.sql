-- Fix Casbin chat rules: paths must include the /api prefix because the
-- /chat scope is nested inside /api in startup.rs (web::scope("/api")
-- contains web::scope("/chat")).  Without the prefix every request to
-- /api/chat/* is denied with 403 by the authorization middleware.

-- ── chat history (/api/chat/history) ────────────────────────────────
DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v1 = '/chat/history';

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
  ('p', 'group_user',  '/api/chat/history', 'GET',    '', '', ''),
  ('p', 'group_user',  '/api/chat/history', 'PUT',    '', '', ''),
  ('p', 'group_user',  '/api/chat/history', 'DELETE', '', '', ''),
  ('p', 'group_admin', '/api/chat/history', 'GET',    '', '', ''),
  ('p', 'group_admin', '/api/chat/history', 'PUT',    '', '', ''),
  ('p', 'group_admin', '/api/chat/history', 'DELETE', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;

-- ── chat sessions CRUD (/api/chat/sessions) ─────────────────────────
DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN ('/chat/sessions', '/chat/sessions/:id', '/chat/sessions/:id/messages');

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
  ('p', 'group_user', '/api/chat/sessions',              'GET',    '', '', ''),
  ('p', 'group_user', '/api/chat/sessions',              'POST',   '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id',          'DELETE', '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id/messages', 'GET',    '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;

-- ── chat sessions append/rename ─────────────────────────────────────
DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND ( (v1 = '/chat/sessions/:id/messages' AND v2 IN ('POST', 'PUT'))
     OR (v1 = '/chat/sessions/:id'          AND v2 = 'PATCH') );

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
  ('p', 'group_user', '/api/chat/sessions/:id/messages', 'POST',  '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id/messages', 'PUT',   '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id',          'PATCH', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;

-- ── chat sessions archive/unarchive ─────────────────────────────────
DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN ('/chat/sessions/:id/archive', '/chat/sessions/:id/unarchive');

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
  ('p', 'group_user', '/api/chat/sessions/:id/archive',   'POST', '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id/unarchive', 'POST', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
