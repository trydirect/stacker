-- Anonymous access to the gateway's public routes (health + ground-truth).
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES
  ('p', 'group_anonymous', '/health', 'GET', '', '', ''),
  ('p', 'group_anonymous', '/public/resolve_image', 'POST', '', '', '')
ON CONFLICT DO NOTHING;
