DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 = '/api/admin/vendors/:creator_user_id/vendor-profile'
  AND v2 = 'PATCH';
