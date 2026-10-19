DELETE FROM public.casbin_rule WHERE ptype = 'p' AND v1 IN ('/project/:id/clouds', '/api/v1/project/:id/clouds');
