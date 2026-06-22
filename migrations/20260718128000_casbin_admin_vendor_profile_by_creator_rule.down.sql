DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN (
      '/api/admin/vendors/:creator_user_id/vendor-profile',
      '/stacker/api/admin/vendors/:creator_user_id/vendor-profile'
  )
  AND v2 = 'PATCH';
