-- Public (anonymous) access to the gateway's read-only ground-truth endpoint.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES
  ('p', 'group_anonymous', '/public/resolve_image', 'POST', '', '', '')
ON CONFLICT DO NOTHING;
