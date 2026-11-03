-- Public (anonymous) access to the /api/audit/* checker endpoints.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES
  ('p', 'group_anonymous', '/api/audit/compose',    'POST', '', '', ''),
  ('p', 'group_anonymous', '/api/audit/dockerfile', 'POST', '', '', ''),
  ('p', 'group_anonymous', '/api/audit/exposure',   'POST', '', '', ''),
  ('p', 'group_anonymous', '/api/audit/readiness',  'POST', '', '', ''),
  ('p', 'group_anonymous', '/api/audit/cost',       'POST', '', '', ''),
  ('p', 'group_anonymous', '/api/audit/image',      'POST', '', '', '');
