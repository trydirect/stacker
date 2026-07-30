DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v0 = 'group_anonymous'
  AND v1 IN ('/health', '/public/resolve_image');
