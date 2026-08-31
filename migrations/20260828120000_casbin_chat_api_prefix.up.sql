-- The chat scope was moved under /api (see startup.rs: web::scope("/api") ->
-- web::scope("/chat")). Casbin matches the FULL request path (other rules use
-- /api/v1/...), so the previous /chat/... rules no longer match /api/chat/...
-- and every chat request was denied with 403. Replace them with /api-prefixed
-- rules. This also fixes the legacy /chat/history rules moved by the same change.

-- Drop the now-stale rules (they match no route after the /api move).
DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN (
    '/chat/history',
    '/chat/sessions',
    '/chat/sessions/:id',
    '/chat/sessions/:id/messages',
    '/chat/sessions/:id/archive',
    '/chat/sessions/:id/unarchive'
  );

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
  -- Legacy single-blob history (user + admin, as originally granted).
  ('p', 'group_user',  '/api/chat/history', 'GET',    '', '', ''),
  ('p', 'group_user',  '/api/chat/history', 'PUT',    '', '', ''),
  ('p', 'group_user',  '/api/chat/history', 'DELETE', '', '', ''),
  ('p', 'group_admin', '/api/chat/history', 'GET',    '', '', ''),
  ('p', 'group_admin', '/api/chat/history', 'PUT',    '', '', ''),
  ('p', 'group_admin', '/api/chat/history', 'DELETE', '', '', ''),
  -- Multi-session endpoints (user-only; group_admin inherits via group_user but
  -- every handler is owner-scoped, so admins only ever touch their own).
  ('p', 'group_user', '/api/chat/sessions',              'GET',    '', '', ''),
  ('p', 'group_user', '/api/chat/sessions',              'POST',   '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id',          'DELETE', '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id',          'PATCH',  '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id/messages', 'GET',    '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id/messages', 'POST',   '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id/messages', 'PUT',    '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id/archive',   'POST',  '', '', ''),
  ('p', 'group_user', '/api/chat/sessions/:id/unarchive', 'POST',  '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
