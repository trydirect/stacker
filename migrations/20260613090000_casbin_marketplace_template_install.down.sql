DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v0 = 'group_user'
  AND v1 IN ('/api/templates/:slug/install', '/api/v1/templates/:slug/install')
  AND v2 = 'POST';
