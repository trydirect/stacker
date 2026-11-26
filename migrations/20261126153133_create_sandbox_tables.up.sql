-- Sandboxes (WS-S S1): short-lived, domain-less, volume-less containers for
-- untrusted and AI-generated code. Lifecycle (inactivity + hard-lifetime) is
-- enforced by the sandbox lifecycle manager.
CREATE TABLE IF NOT EXISTS sandbox (
    id UUID PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    project_id INTEGER REFERENCES project(id) ON DELETE CASCADE,
    -- Where it runs (the agent addressed for exec/kill).
    deployment_id INTEGER REFERENCES deployment(id) ON DELETE CASCADE,
    status VARCHAR(20) NOT NULL DEFAULT 'creating',
    image TEXT NOT NULL,
    runtime VARCHAR(20) NOT NULL DEFAULT 'runc',
    network_mode VARCHAR(20) NOT NULL DEFAULT 'isolated',
    cpu DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    mem_mb INTEGER NOT NULL DEFAULT 512,
    pids_limit INTEGER NOT NULL DEFAULT 128,
    inactivity_timeout_sec INTEGER NOT NULL DEFAULT 1800,
    max_lifetime_sec INTEGER NOT NULL DEFAULT 86400,
    -- Env is write-only: encrypted at rest, never returned by the API (S3).
    env_encrypted TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '24 hours',
    CONSTRAINT sandbox_status_check CHECK (
        status IN ('creating', 'running', 'killed', 'failed', 'expired')
    ),
    CONSTRAINT sandbox_runtime_check CHECK (runtime IN ('runc', 'runsc')),
    CONSTRAINT sandbox_network_check CHECK (network_mode IN ('isolated', 'none')),
    CONSTRAINT sandbox_limits_check CHECK (
        cpu > 0 AND mem_mb > 0 AND pids_limit > 0
    )
);

CREATE INDEX IF NOT EXISTS idx_sandbox_user ON sandbox (user_id);
CREATE INDEX IF NOT EXISTS idx_sandbox_project ON sandbox (project_id);
CREATE INDEX IF NOT EXISTS idx_sandbox_status ON sandbox (status);
CREATE INDEX IF NOT EXISTS idx_sandbox_expires ON sandbox (expires_at);
