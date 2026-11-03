DELETE FROM public.casbin_rule WHERE ptype = 'p' AND v1 = '/api/agreement/name/:name' AND v2 = 'GET';
DELETE FROM public.casbin_rule
WHERE ptype = 'p' AND v0 = 'group_admin' AND v1 = '/api/admin/chat/recent' AND v2 = 'GET';
DELETE FROM user_agreement WHERE agrt_id IN (SELECT id FROM agreement WHERE name = 'ai_chat_analysis');
DELETE FROM agreement WHERE name = 'ai_chat_analysis';
