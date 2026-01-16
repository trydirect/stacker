INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'group_user', '/dockerhub/namespaces', 'GET', '', '', '');

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'group_admin', '/dockerhub/namespaces', 'GET', '', '', '');

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'group_user', '/dockerhub/:namespace/repositories', 'GET', '', '', '');

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'group_admin', '/dockerhub/:namespace/repositories', 'GET', '', '', '');

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'group_user', '/dockerhub/:namespace/repositories/:repository/tags', 'GET', '', '', '');

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5)
VALUES ('p', 'group_admin', '/dockerhub/:namespace/repositories/:repository/tags', 'GET', '', '', '');
