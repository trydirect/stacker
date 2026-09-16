DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v0 IN ('group_user', 'group_admin')
  AND v1 IN ('/project/:id/sync', '/api/v1/project/:id/sync')
  AND v2 = 'PUT';
