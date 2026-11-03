-- Allow authenticated users to mint CLI account handoff commands.
-- Route /api/v1/handoff/mint/account (added alongside /api/v1/handoff/mint
-- in src/routes/handoff/mod.rs) was missing a Casbin policy rule, causing
-- authenticated requests to be rejected with 403.
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'group_user', '/api/v1/handoff/mint/account', 'POST', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
