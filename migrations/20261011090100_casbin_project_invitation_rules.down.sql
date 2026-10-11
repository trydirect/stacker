DELETE FROM public.casbin_rule WHERE ptype = 'p' AND v1 IN (
  '/project/:id/invitations',
  '/project/:id/invitations/:invitation_id',
  '/project/:id/seats',
  '/project/invitations/pending',
  '/project/invitations/:key/accept',
  '/project/invitations/:token',
  '/api/v1/project/:id/invitations',
  '/api/v1/project/:id/invitations/:invitation_id',
  '/api/v1/project/:id/seats',
  '/api/v1/project/invitations/pending',
  '/api/v1/project/invitations/:key/accept',
  '/api/v1/project/invitations/:token');
-- The POST .../members rules stay: /project/:id/members POST predates this
-- migration, and removing it would break adding members by id.
