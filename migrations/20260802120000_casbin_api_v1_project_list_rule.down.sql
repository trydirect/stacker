DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v0 = 'group_user' AND v1 = '/api/v1/project' AND v2 = 'GET';

DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v0 = 'group_user' AND v1 = '/api/v1/project' AND v2 = 'POST';

DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v0 = 'group_admin' AND v1 = '/api/v1/project' AND v2 = 'GET';
