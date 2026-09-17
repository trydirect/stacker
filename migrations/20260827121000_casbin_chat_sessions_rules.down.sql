DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v1 IN ('/chat/sessions', '/chat/sessions/:id', '/chat/sessions/:id/messages');
