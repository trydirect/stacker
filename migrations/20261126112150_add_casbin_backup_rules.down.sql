-- Remove Casbin rules for /api/v1/backups endpoints
DELETE FROM public.casbin_rule
WHERE v1 IN (
    '/api/v1/backups/destinations',
    '/api/v1/backups/destinations/:id',
    '/api/v1/backups/jobs',
    '/api/v1/backups/jobs/:id',
    '/api/v1/backups/jobs/:id/runs'
);
