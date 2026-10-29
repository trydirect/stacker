DELETE FROM public.casbin_rule WHERE ptype = 'p' AND v1 = '/api/admin/bake/:id/promote';
ALTER TABLE baked_snapshots DROP COLUMN IF EXISTS gated;
DROP TABLE IF EXISTS baked_snapshot_channels;
