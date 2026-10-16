-- Daily usage of the Managed server behind a Quick Deploy: one row per
-- deployment. The sweeper reports each running day to the Stripe meter
-- through the User Service and records here how far it got.
-- Contract: config/shared-fixtures/api-contracts/managed-server-billing.json.
CREATE TABLE IF NOT EXISTS managed_usage (
    id                    BIGSERIAL    PRIMARY KEY,
    deployment_hash       VARCHAR(120) NOT NULL UNIQUE,
    user_id               VARCHAR(64)  NOT NULL,
    stack                 VARCHAR(255) NOT NULL,
    server_type           VARCHAR(50)  NOT NULL,
    daily_rate_minor      BIGINT       NOT NULL CHECK (daily_rate_minor > 0),
    monthly_cap_minor     BIGINT       NOT NULL CHECK (monthly_cap_minor > 0),
    currency              CHAR(3)      NOT NULL DEFAULT 'USD',
    -- Hetzner server id, set when the clone creates the VM; admins delete the
    -- VM by hand from the alert (nothing deletes it automatically).
    server_id             BIGINT,
    -- pending: deploy running; active: billed daily; stopped: server deleted;
    -- suspended: invoice unpaid; closed: deploy failed, or unpaid and handed
    -- to admins for deletion
    status                VARCHAR(16)  NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'active', 'stopped', 'suspended', 'closed')),
    outcome               VARCHAR(40),
    started_at            TIMESTAMPTZ,
    stopped_at            TIMESTAMPTZ,
    suspended_at          TIMESTAMPTZ,
    period_start          DATE,
    period_reported_minor BIGINT       NOT NULL DEFAULT 0,
    last_reported_on      DATE,
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS ix_managed_usage_status ON managed_usage (status, last_reported_on);
CREATE INDEX IF NOT EXISTS ix_managed_usage_user ON managed_usage (user_id, status);
