CREATE TABLE IF NOT EXISTS github_installation (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    installation_id BIGINT NOT NULL,
    account_login VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_github_installation_user_unique
    ON github_installation (user_id, installation_id);

CREATE TABLE IF NOT EXISTS github_repo (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    installation_id BIGINT NOT NULL,
    project_id INTEGER REFERENCES project(id) ON DELETE CASCADE,
    repo VARCHAR(255) NOT NULL,
    branch VARCHAR(255) NOT NULL DEFAULT 'main',
    watch_paths TEXT[] NOT NULL DEFAULT '{}',
    autodeploy BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_github_repo_unique
    ON github_repo (user_id, repo, branch);

CREATE INDEX IF NOT EXISTS idx_github_repo_project
    ON github_repo (project_id);

CREATE INDEX IF NOT EXISTS idx_github_repo_push_lookup
    ON github_repo (repo, branch)
    WHERE autodeploy;

CREATE TABLE IF NOT EXISTS github_webhook_delivery (
    delivery_id VARCHAR(128) PRIMARY KEY,
    event_type VARCHAR(64) NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
