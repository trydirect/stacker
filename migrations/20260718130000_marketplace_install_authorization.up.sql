CREATE TABLE marketplace_install_authorization (
    id                  uuid          PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id          integer,
    user_id             varchar(64)   NOT NULL,
    template_id         uuid          NOT NULL REFERENCES stack_template(id) ON DELETE RESTRICT,
    idempotency_key     varchar(80)   NOT NULL,
    authorization_id    varchar(120)  NOT NULL,
    amount_minor        bigint        NOT NULL,
    currency            char(3)       NOT NULL,
    status              varchar(24)   NOT NULL,
    deployment_hash     varchar(120),
    void_reason         varchar(120),
    expires_at          timestamptz,
    created_at          timestamptz   NOT NULL DEFAULT now(),
    updated_at          timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT uq_mia_idem UNIQUE (user_id, idempotency_key)
);

CREATE INDEX ix_mia_project
    ON marketplace_install_authorization (project_id);

CREATE INDEX ix_mia_deploy_hash
    ON marketplace_install_authorization (deployment_hash)
    WHERE deployment_hash IS NOT NULL;

CREATE INDEX ix_mia_sweep
    ON marketplace_install_authorization (status, expires_at)
    WHERE status = 'authorized';

CREATE INDEX ix_mia_auth_id
    ON marketplace_install_authorization (authorization_id);
