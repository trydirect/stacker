DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND ( (v1 = '/chat/sessions/:id/messages' AND v2 = 'POST')
     OR (v1 = '/chat/sessions/:id'          AND v2 = 'PATCH') );
