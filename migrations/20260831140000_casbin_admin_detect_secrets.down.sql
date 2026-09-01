-- Remove Casbin rules for admin template detect-secrets endpoint
DELETE FROM public.casbin_rule WHERE ptype = 'p' AND v1 = '/api/admin/templates/:id/detect-secrets' AND v2 = 'POST';
DELETE FROM public.casbin_rule WHERE ptype = 'p' AND v1 = '/stacker/admin/templates/:id/detect-secrets' AND v2 = 'POST';
