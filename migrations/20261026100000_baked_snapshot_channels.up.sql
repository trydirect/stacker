-- Release channels for baked snapshots (BAKE_PIPELINE_PLAN.md, Stage 4): the
-- pointers helpers/bake_registry.rs moves. A bake started from the API lands on
-- `unstable`; buyers get `latest` (promoted after a green smoke clone) or, when
-- that is unhealthy, `stable` (the previous `latest`).
CREATE TABLE IF NOT EXISTS baked_snapshot_channels (
    stack       TEXT NOT NULL,
    provider    TEXT NOT NULL,
    channel     TEXT NOT NULL,
    snapshot_id INTEGER NOT NULL REFERENCES baked_snapshots (id),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (stack, provider, channel)
);

CREATE INDEX IF NOT EXISTS idx_baked_snapshot_channels_snapshot
    ON baked_snapshot_channels (snapshot_id);

-- A gated snapshot reaches buyers only through `latest` or `stable`. Rows
-- recorded before this migration, and by the `bake` binary, stay ungated and
-- keep the "newest healthy" behaviour.
ALTER TABLE baked_snapshots ADD COLUMN IF NOT EXISTS gated BOOLEAN NOT NULL DEFAULT FALSE;

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/admin/bake/:id/promote', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'admin_service', '/api/admin/bake/:id/promote', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
