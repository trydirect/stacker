-- Add Casbin rule for the GitHub App webhook receiver (anonymous; the route
-- authenticates the caller by the X-Hub-Signature-256 HMAC instead).
-- Precedent: 20260718124000_casbin_payout_webhook_rule.

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES
    ('p', 'group_anonymous', '/api/v1/github/webhook', 'POST', '', '', '')
ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
