-- Append-message and rename endpoints for chat sessions (user-only, same
-- rationale as 20260827121000: group_admin inherits these via group_user, but
-- every handler is owner-scoped so an admin only ever touches their own).
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
  ('p', 'group_user', '/chat/sessions/:id/messages', 'POST',  '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id',          'PATCH', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
