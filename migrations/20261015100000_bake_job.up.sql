-- Bake started from the API: one row per POST /api/admin/bake.
CREATE TABLE IF NOT EXISTS bake_job (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    stack TEXT NOT NULL,
    version TEXT NOT NULL,
    deployment_id INTEGER NOT NULL,
    server_id INTEGER NOT NULL,
    server_ip TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'done', 'failed')),
    image_id BIGINT,
    snapshot_id INTEGER,
    error TEXT,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS bake_job_created_at_idx ON bake_job (created_at DESC);

INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/admin/bake', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'group_admin', '/api/admin/bake/:id', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'admin_service', '/api/admin/bake', 'POST', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
INSERT INTO public.casbin_rule (ptype, v0, v1, v2, v3, v4, v5) VALUES ('p', 'admin_service', '/api/admin/bake/:id', 'GET', '', '', '') ON CONFLICT ON CONSTRAINT unique_key_sqlx_adapter DO NOTHING;
