-- The /api/v1/project alias route (see src/startup.rs: both project_scope("/project")
-- and project_scope("/api/v1/project") are registered) has been missing Casbin policy
-- rules for most sub-paths since the alias was introduced (commit 81168cf8) and a later
-- "fix" migration (20260218100000) mistakenly deleted rather than duplicated some of
-- them. This has caused repeated, one-off 403s on the alias route (e.g. project list,
-- container discovery). Instead of continuing to patch individual routes as they're
-- discovered, mirror every existing '/project/...' rule under '/api/v1/project/...'
-- in one pass, skipping any that already exist.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
SELECT ptype, v0, '/api/v1' || v1, v2, v3, v4, v5
FROM public.casbin_rule src
WHERE ptype = 'p'
  AND (v1 = '/project' OR v1 LIKE '/project/%')
  AND v1 NOT LIKE '/project_app%'
  AND NOT EXISTS (
    SELECT 1 FROM public.casbin_rule dup
    WHERE dup.ptype = src.ptype
      AND dup.v0 = src.v0
      AND dup.v1 = '/api/v1' || src.v1
      AND dup.v2 = src.v2
      AND dup.v3 = src.v3
      AND dup.v4 = src.v4
      AND dup.v5 = src.v5
  )
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
