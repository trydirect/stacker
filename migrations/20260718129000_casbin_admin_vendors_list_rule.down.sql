DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN ('/api/admin/vendors', '/stacker/api/admin/vendors')
  AND v2 = 'GET';
