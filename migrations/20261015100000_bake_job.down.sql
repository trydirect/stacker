DELETE FROM public.casbin_rule WHERE ptype = 'p' AND v1 IN ('/api/admin/bake', '/api/admin/bake/:id');
DROP TABLE IF EXISTS bake_job;
