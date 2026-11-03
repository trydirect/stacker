DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN (
      '/api/admin/notifications',
      '/stacker/api/admin/notifications',
      '/api/admin/notifications/unread-count',
      '/stacker/api/admin/notifications/unread-count',
      '/api/admin/notifications/:id',
      '/stacker/api/admin/notifications/:id'
  );
