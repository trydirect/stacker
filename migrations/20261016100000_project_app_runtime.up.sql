-- Compose settings of an app beyond its image and wiring: cap_add, cap_drop,
-- devices, profiles, aliases, deploy (replicas, device reservations).
-- See shared-fixtures/api-contracts/stack-service-runtime-fields.json.
ALTER TABLE project_app ADD COLUMN IF NOT EXISTS runtime JSONB;
