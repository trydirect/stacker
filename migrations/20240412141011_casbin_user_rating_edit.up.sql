-- Add up migration script here
BEGIN TRANSACTION;

INSERT INTO casbin_rule
(id, ptype, v0, v1, v2, v3, v4, v5)
VALUES((select max(id) + 1 from casbin_rule cr), 'p', 'group_user', '/rating/:id', 'PUT', '', '', '');

INSERT INTO casbin_rule
(id, ptype, v0, v1, v2, v3, v4, v5)
VALUES((select max(id) + 1 from casbin_rule cr), 'p', 'group_admin', '/admin/rating/:id', 'PUT', '', '', '');

INSERT INTO casbin_rule
(id, ptype, v0, v1, v2, v3, v4, v5)
VALUES((select max(id) + 1 from casbin_rule cr), 'p', 'group_user', '/rating/:id', 'DELETE', '', '', '');

INSERT INTO casbin_rule
(id, ptype, v0, v1, v2, v3, v4, v5)
VALUES((select max(id) + 1 from casbin_rule cr), 'p', 'group_admin', '/admin/rating/:id', 'GET', '', '', '');

INSERT INTO casbin_rule
(id, ptype, v0, v1, v2, v3, v4, v5)
VALUES((select max(id) + 1 from casbin_rule cr), 'p', 'group_admin', '/admin/rating', 'GET', '', '', '');

COMMIT TRANSACTION;
