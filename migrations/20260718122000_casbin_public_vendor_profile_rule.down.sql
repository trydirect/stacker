DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v0 = 'group_anonymous'
  AND v1 = '/api/vendors/:vendor'
  AND v2 = 'GET';
