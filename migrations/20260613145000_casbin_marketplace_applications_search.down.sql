DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v0 = 'group_user'
  AND v1 = '/api/v1/marketplace/applications'
  AND v2 = 'GET';
