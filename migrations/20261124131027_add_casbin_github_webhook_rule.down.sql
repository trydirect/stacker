-- Remove the Casbin rule for the GitHub App webhook receiver.

DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v0 = 'group_anonymous'
  AND v1 = '/api/v1/github/webhook'
  AND v2 = 'POST';
