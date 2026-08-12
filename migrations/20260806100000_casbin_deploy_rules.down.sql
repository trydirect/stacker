DELETE FROM public.casbin_rule
WHERE v1 IN ('/api/v1/deploy/validate', '/api/v1/deploy/clone');
