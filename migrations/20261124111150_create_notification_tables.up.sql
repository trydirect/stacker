CREATE TABLE IF NOT EXISTS notification_channel (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    project_id INTEGER REFERENCES project(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    channel_type VARCHAR(20) NOT NULL,
    vault_path TEXT NOT NULL,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    events TEXT[] NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT notification_channel_type_check CHECK (
        channel_type IN ('slack', 'discord', 'telegram', 'email', 'webhook')
    )
);

-- per-project channel names are unique per user; NULL project = global admin channel
CREATE UNIQUE INDEX IF NOT EXISTS idx_notification_channel_project_unique
    ON notification_channel (user_id, project_id, name)
    WHERE project_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_notification_channel_global_unique
    ON notification_channel (user_id, name)
    WHERE project_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_notification_channel_project
    ON notification_channel (project_id)
    WHERE project_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notification_channel_enabled
    ON notification_channel (user_id, enabled);

CREATE TABLE IF NOT EXISTS notification_dispatch (
    id SERIAL PRIMARY KEY,
    channel_id INTEGER NOT NULL REFERENCES notification_channel(id) ON DELETE CASCADE,
    event_type VARCHAR(50) NOT NULL,
    project_id INTEGER REFERENCES project(id) ON DELETE CASCADE,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sent_at TIMESTAMPTZ,
    CONSTRAINT notification_dispatch_status_check CHECK (
        status IN ('pending', 'sent', 'failed', 'skipped')
    )
);

CREATE INDEX IF NOT EXISTS idx_notification_dispatch_channel
    ON notification_dispatch (channel_id, created_at);

CREATE INDEX IF NOT EXISTS idx_notification_dispatch_status
    ON notification_dispatch (status)
    WHERE status IN ('pending', 'failed');

CREATE INDEX IF NOT EXISTS idx_notification_dispatch_event
    ON notification_dispatch (event_type, created_at);
