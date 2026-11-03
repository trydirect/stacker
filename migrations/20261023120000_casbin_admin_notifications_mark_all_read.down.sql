DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v2 = 'POST'
  AND v1 IN (
      '/api/admin/notifications/mark-all-read',
      '/stacker/api/admin/notifications/mark-all-read'
  );
