-- What has actually been seen running on a deployment.
--
-- Stacker had no container table at all: the dashboard's list was recomputed on
-- every request from the newest health report, so a report that was late,
-- partial or of the wrong shape changed what the user saw. Containers appeared,
-- vanished after a reload, and came back on their own. Three separate causes
-- were fixed one by one; this removes the category.
--
-- Presence is unambiguous, absence is not: a container missing from one report
-- may be gone, or docker may have hiccuped while listing. So a row is written
-- when a container is seen and retired only on an explicit event — the app was
-- removed. Silence merely stops refreshing `last_seen_at`, and the snapshot
-- reports the row as stale rather than dropping it.
CREATE TABLE IF NOT EXISTS deployment_container (
    id              SERIAL PRIMARY KEY,
    deployment_hash VARCHAR NOT NULL,
    -- The key, rather than the app code: platform containers report codes like
    -- `web` and `agent`, or no code at all, but a name is always present.
    container_name  VARCHAR NOT NULL,
    app_code        VARCHAR,
    -- `project` or `platform`, from helpers::stacker_labels. Refreshed on every
    -- report, so a container that gains a `my.stacker.scope` label is
    -- reclassified without operator action.
    scope           VARCHAR NOT NULL DEFAULT 'project',
    image           VARCHAR,
    state           VARCHAR,
    -- TIMESTAMPTZ throughout: the models map these to DateTime<Utc>, which
    -- cannot decode a naive column, and because query_as resolves types at
    -- runtime the mismatch would surface as a bare "Database error" at request
    -- time rather than a compile failure. See 20260907120000_agents_token_hash.
    first_seen_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set when the app is removed on purpose. Rows are kept afterwards so the
    -- dashboard can say a container went away rather than silently losing it.
    removed_at      TIMESTAMPTZ,
    CONSTRAINT deployment_container_unique UNIQUE (deployment_hash, container_name)
);

-- The snapshot's only query: every live container of one deployment.
CREATE INDEX IF NOT EXISTS deployment_container_by_deployment
    ON deployment_container (deployment_hash)
    WHERE removed_at IS NULL;

COMMENT ON TABLE deployment_container IS
  'Containers observed on a deployment. Membership of the dashboard list comes from here, so it changes only on an explicit event, not on a missing or partial agent report.';
COMMENT ON COLUMN deployment_container.last_seen_at IS
  'When an aggregate health report last mentioned this container. Staleness is derived from it; absence never deletes a row.';
COMMENT ON COLUMN deployment_container.removed_at IS
  'Set when remove_app completed for this container''s app. Rows are swept later, not on the spot.';
