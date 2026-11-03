DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v0 IN ('group_anonymous', 'group_user', 'group_admin')
  AND v1 IN (
      '/api/audit/compose',
      '/api/audit/dockerfile',
      '/api/audit/exposure',
      '/api/audit/readiness',
      '/api/audit/cost',
      '/api/audit/image'
  )
  AND v2 = 'POST';
