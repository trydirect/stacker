-- Add down migration script here
DELETE FROM casbin_rule
WHERE ptype = 'p' and v0 = 'group_user' and v1 = '/rating/:id' and v2 = 'PUT';
