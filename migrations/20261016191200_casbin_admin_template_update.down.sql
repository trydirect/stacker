DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v0 IN ('admin_service', 'group_admin', 'root')
  AND v1 IN ('/api/admin/templates/:id', '/stacker/admin/templates/:id')
  AND v2 = 'PUT';
