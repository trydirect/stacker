-- Consent agreement for analysing AI chat conversations. Admin chat review
-- (GET /api/admin/chat/recent) only returns chats of users who accepted it.
INSERT INTO agreement (name, text, created_at, updated_at)
SELECT 'ai_chat_analysis',
       'Allow TryDirect to analyze my AI chat conversations to improve our AI assistant and services. Conversations are reviewed only by authorized TryDirect staff. You can withdraw this consent at any time by contacting support.',
       NOW(), NOW()
WHERE NOT EXISTS (SELECT 1 FROM agreement WHERE name = 'ai_chat_analysis');

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
SELECT 'p', 'group_admin', '/api/admin/chat/recent', 'GET', '', '', ''
WHERE NOT EXISTS (
    SELECT 1 FROM public.casbin_rule
    WHERE ptype = 'p' AND v0 = 'group_admin' AND v1 = '/api/admin/chat/recent' AND v2 = 'GET'
);

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
SELECT 'p', 'group_user', '/api/agreement/name/:name', 'GET', '', '', ''
WHERE NOT EXISTS (
    SELECT 1 FROM public.casbin_rule
    WHERE ptype = 'p' AND v0 = 'group_user' AND v1 = '/api/agreement/name/:name' AND v2 = 'GET'
);

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
SELECT 'p', 'group_admin', '/api/agreement/name/:name', 'GET', '', '', ''
WHERE NOT EXISTS (
    SELECT 1 FROM public.casbin_rule
    WHERE ptype = 'p' AND v0 = 'group_admin' AND v1 = '/api/agreement/name/:name' AND v2 = 'GET'
);
