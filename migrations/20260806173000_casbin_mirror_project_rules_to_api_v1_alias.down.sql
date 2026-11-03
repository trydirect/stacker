DELETE FROM public.casbin_rule dst
WHERE dst.ptype = 'p'
  AND dst.v1 LIKE '/api/v1/project%'
  AND EXISTS (
    SELECT 1 FROM public.casbin_rule src
    WHERE src.ptype = dst.ptype
      AND src.v0 = dst.v0
      AND src.v1 = substring(dst.v1 from 8)
      AND src.v2 = dst.v2
      AND src.v3 = dst.v3
      AND src.v4 = dst.v4
      AND src.v5 = dst.v5
  );
