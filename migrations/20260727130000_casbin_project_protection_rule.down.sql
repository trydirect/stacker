DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v1 = '/project/:id/protection' AND v2 = 'PATCH';

DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v1 = '/api/v1/project/:id/protection' AND v2 = 'PATCH';
