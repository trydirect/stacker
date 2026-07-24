DELETE FROM public.casbin_rule
WHERE v0 IN ('group_user', 'group_admin')
  AND v1 = '/api/templates/mine/vendor-profile'
  AND v2 = 'PATCH';
