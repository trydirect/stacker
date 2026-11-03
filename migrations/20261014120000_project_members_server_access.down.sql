DELETE FROM public.casbin_rule WHERE ptype = 'p' AND v1 IN ('/project/:id/server-access', '/api/v1/project/:id/server-access');
ALTER TABLE project DROP COLUMN IF EXISTS members_server_access;
