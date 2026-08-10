CREATE TABLE IF NOT EXISTS baked_snapshots (
    id          SERIAL PRIMARY KEY,
    stack       TEXT NOT NULL,
    version     TEXT NOT NULL,
    provider    TEXT NOT NULL DEFAULT 'hetzner',
    image_id    BIGINT NOT NULL,
    healthy     BOOLEAN NOT NULL DEFAULT TRUE,
    digests     JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_baked_snapshots_resolve
    ON baked_snapshots (stack, version, provider, healthy, created_at DESC);
