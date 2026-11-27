-- Remove Casbin rules for /api/v1/notifications endpoints

DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v0 IN ('group_user', 'root')
  AND v1 LIKE '/api/v1/notifications/%';
