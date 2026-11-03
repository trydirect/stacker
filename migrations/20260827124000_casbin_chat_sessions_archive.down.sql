DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN ('/chat/sessions/:id/archive', '/chat/sessions/:id/unarchive');
