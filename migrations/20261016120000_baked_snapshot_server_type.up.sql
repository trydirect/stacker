-- Server type of the build box a snapshot was baked on. Quick Deploy clones the
-- snapshot onto that type and prices it at that type's rate.
-- See shared-fixtures/api-contracts/quick-deploy-server-type.json.
ALTER TABLE baked_snapshots ADD COLUMN IF NOT EXISTS server_type TEXT;
