DELETE FROM public.casbin_rule WHERE ptype = 'p' AND v0 = 'group_anonymous' AND v1 IN (
    '/api/internal/managed-usage/:hash/start',
    '/api/internal/managed-usage/:hash/fail',
    '/api/internal/managed-usage/:hash/stop',
    '/api/internal/managed-usage/suspend'
) AND v2 = 'POST';
