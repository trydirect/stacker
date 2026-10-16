-- Internal Managed server usage endpoints, called by the User Service with
-- no user token. Casbin only makes them reachable; the handlers authorise
-- with require_internal_key (x-internal-key).
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_anonymous', '/api/internal/managed-usage/:hash/start', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_anonymous', '/api/internal/managed-usage/:hash/fail', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_anonymous', '/api/internal/managed-usage/suspend', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
