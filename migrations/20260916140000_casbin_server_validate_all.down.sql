DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 = '/server/ssh-key/validate-all'
  AND v2 = 'POST';
