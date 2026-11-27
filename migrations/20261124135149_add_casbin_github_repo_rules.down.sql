-- Remove the Casbin rules for the GitHub Repo Connect endpoints.

DELETE FROM public.casbin_rule
WHERE ptype = 'p'
  AND v0 IN ('group_user', 'root')
  AND v1 LIKE '/api/v1/github/repos%';
