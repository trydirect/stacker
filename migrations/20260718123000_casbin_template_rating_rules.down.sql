DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN (
    '/api/templates/:id/rating/summary',
    '/api/templates/:id/rating/me',
    '/api/templates/:id/rating'
  )
  AND v2 IN ('GET', 'PUT', 'DELETE');
