-- Archive / unarchive endpoints for chat sessions (user-only, owner-scoped;
-- same inheritance note as 20260827121000).
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
  ('p', 'group_user', '/chat/sessions/:id/archive',   'POST', '', '', ''),
  ('p', 'group_user', '/chat/sessions/:id/unarchive', 'POST', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
