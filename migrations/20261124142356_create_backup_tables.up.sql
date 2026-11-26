CREATE TABLE IF NOT EXISTS backup_destination (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    endpoint TEXT NOT NULL,
    vault_path TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_backup_destination_user_name
    ON backup_destination (user_id, name);

CREATE TABLE IF NOT EXISTS backup_job (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    project_id INTEGER NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    destination_id INTEGER NOT NULL REFERENCES backup_destination(id),
    name VARCHAR(255) NOT NULL,
    source VARCHAR(20) NOT NULL,
    target VARCHAR(255) NOT NULL,
    database VARCHAR(255),
    schedule VARCHAR(100) NOT NULL DEFAULT '0 3 * * *',
    retention_daily INTEGER NOT NULL DEFAULT 7,
    retention_weekly INTEGER NOT NULL DEFAULT 4,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT backup_job_source_check CHECK (
        source IN ('postgres', 'mysql', 'mongo', 'volume')
    ),
    CONSTRAINT backup_job_retention_check CHECK (
        retention_daily >= 0 AND retention_weekly >= 0
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_backup_job_user_project_name
    ON backup_job (user_id, project_id, name);

CREATE INDEX IF NOT EXISTS idx_backup_job_project
    ON backup_job (project_id);

CREATE TABLE IF NOT EXISTS backup_run (
    id SERIAL PRIMARY KEY,
    job_id INTEGER NOT NULL REFERENCES backup_job(id) ON DELETE CASCADE,
    project_id INTEGER NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    size_bytes BIGINT,
    checksum VARCHAR(128),
    remote_file_id VARCHAR(255),
    error TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT backup_run_status_check CHECK (
        status IN ('pending', 'running', 'uploaded', 'failed', 'pruned', 'restored')
    )
);

CREATE INDEX IF NOT EXISTS idx_backup_run_job
    ON backup_run (job_id, created_at);

CREATE INDEX IF NOT EXISTS idx_backup_run_status
    ON backup_run (status);
