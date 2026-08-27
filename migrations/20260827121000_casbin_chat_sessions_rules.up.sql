-- Chat sessions are a user-only feature: only the `group_user` role gets CRUD.
-- NOTE: `group_admin` inherits `group_user` (see 20260101090000), and this
-- casbin model is allow-only (no deny effect), so admins technically inherit
-- these grants. That is acceptable here because every chat-session handler
-- scopes its query by the authenticated user_id — an admin only ever sees their
-- OWN sessions, never another user's. There is deliberately NO admin-specific
-- grant (no moderation/deactivate endpoint yet).
--
-- Object patterns use keyMatch2 (`:id` -> `[^/]+`), matching the role manager's
-- matching_fn configured in src/middleware/authorization.rs.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
  ('p', 'group_user', '/chat/sessions',              'GET',    '', '', ''),
  ('p', 'group_user', '/chat/sessions',              'POST',   '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id',          'DELETE', '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id/messages', 'GET',    '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
