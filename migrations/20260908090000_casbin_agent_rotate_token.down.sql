DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 = '/api/v1/agent/rotate-token/*'
  AND v2 = 'POST';
