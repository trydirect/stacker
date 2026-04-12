DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v0 = 'group_user' AND v1 = '/project/:id/rollback' AND v2 = 'POST';

DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v0 = 'client' AND v1 = '/project/:id/rollback' AND v2 = 'POST';
