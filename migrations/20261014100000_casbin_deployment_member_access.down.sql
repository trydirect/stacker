DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v1 = '/api/v1/deployments/:deployment_hash/member-access';
