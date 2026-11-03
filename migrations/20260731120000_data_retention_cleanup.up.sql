-- Data retention cleanup: pg_cron functions for purging stale data.
-- Follows the pattern of stacker_command_queue_cleanup (migration 20260312210000).
-- All functions default to dry-run mode (p_dry_run = true) — log candidates only.
-- Pass p_dry_run := false to enable actual deletion.

-- Cleanup log table for audit trail
CREATE TABLE IF NOT EXISTS cleanup_log (
    id BIGSERIAL PRIMARY KEY,
    run_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    function_name TEXT NOT NULL,
    table_name TEXT NOT NULL,
    rows_deleted BIGINT NOT NULL DEFAULT 0,
    retention INTERVAL,
    dry_run BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX IF NOT EXISTS idx_cleanup_log_run_at ON cleanup_log (run_at DESC);


-- 1. Audit logs retention (90 days)
CREATE OR REPLACE FUNCTION stacker_cleanup_audit_logs(
    retention INTERVAL DEFAULT INTERVAL '90 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM agent_audit_log WHERE received_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_audit_logs', 'agent_audit_log', v_count, retention, true);

        SELECT count(*) INTO v_count FROM audit_log WHERE created_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_audit_logs', 'audit_log', v_count, retention, true);

        SELECT count(*) INTO v_count FROM cdc_events WHERE captured_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_audit_logs', 'cdc_events', v_count, retention, true);
    ELSE
        DELETE FROM agent_audit_log WHERE received_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_audit_logs', 'agent_audit_log', v_count, retention, false);

        DELETE FROM audit_log WHERE created_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_audit_logs', 'audit_log', v_count, retention, false);

        DELETE FROM cdc_events WHERE captured_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_audit_logs', 'cdc_events', v_count, retention, false);
    END IF;
END;
$$;


-- 2. Terminal commands retention (30 days)
CREATE OR REPLACE FUNCTION stacker_cleanup_terminal_commands(
    retention INTERVAL DEFAULT INTERVAL '30 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM commands
        WHERE status IN ('completed','failed','cancelled')
          AND COALESCE(completed_at, updated_at) < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_terminal_commands', 'commands', v_count, retention, true);

        SELECT count(*) INTO v_count FROM dead_letter_queue
        WHERE status IN ('exhausted','discarded','resolved')
          AND updated_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_terminal_commands', 'dead_letter_queue', v_count, retention, true);

        SELECT count(*) INTO v_count FROM pipe_executions
        WHERE completed_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_terminal_commands', 'pipe_executions', v_count, retention, true);

        SELECT count(*) INTO v_count FROM pipe_dag_step_executions
        WHERE completed_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_terminal_commands', 'pipe_dag_step_executions', v_count, retention, true);
    ELSE
        -- Delete command_queue entries for terminal commands first (FK)
        DELETE FROM command_queue WHERE command_id IN (
            SELECT id FROM commands
            WHERE status IN ('completed','failed','cancelled')
              AND COALESCE(completed_at, updated_at) < NOW() - retention
        );

        DELETE FROM commands
        WHERE status IN ('completed','failed','cancelled')
          AND COALESCE(completed_at, updated_at) < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_terminal_commands', 'commands', v_count, retention, false);

        DELETE FROM dead_letter_queue
        WHERE status IN ('exhausted','discarded','resolved')
          AND updated_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_terminal_commands', 'dead_letter_queue', v_count, retention, false);

        -- FK-safe order: step executions before executions
        DELETE FROM pipe_dag_step_executions WHERE completed_at < NOW() - retention;

        DELETE FROM pipe_executions WHERE completed_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_terminal_commands', 'pipe_executions', v_count, retention, false);
    END IF;
END;
$$;


-- 3. Soft-deleted deployments purge (90 days)
CREATE OR REPLACE FUNCTION stacker_cleanup_deleted_deployments(
    retention INTERVAL DEFAULT INTERVAL '90 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM deployment
        WHERE deleted = true AND updated_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_deleted_deployments', 'deployment', v_count, retention, true);
    ELSE
        DELETE FROM deployment
        WHERE deleted = true AND updated_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_deleted_deployments', 'deployment', v_count, retention, false);
    END IF;
END;
$$;


-- 4. Stale projects cleanup (90 days, no active deployments/servers, not protected)
CREATE OR REPLACE FUNCTION stacker_cleanup_stale_projects(
    retention INTERVAL DEFAULT INTERVAL '90 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM project p
        WHERE p.is_protected = false
          AND p.updated_at < NOW() - retention
          AND NOT EXISTS (
              SELECT 1 FROM deployment d
              WHERE d.project_id = p.id AND (d.deleted = false OR d.deleted IS NULL)
          )
          AND NOT EXISTS (SELECT 1 FROM server s WHERE s.project_id = p.id);
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_stale_projects', 'project', v_count, retention, true);
    ELSE
        DELETE FROM project p
        WHERE p.is_protected = false
          AND p.updated_at < NOW() - retention
          AND NOT EXISTS (
              SELECT 1 FROM deployment d
              WHERE d.project_id = p.id AND (d.deleted = false OR d.deleted IS NULL)
          )
          AND NOT EXISTS (SELECT 1 FROM server s WHERE s.project_id = p.id);
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_stale_projects', 'project', v_count, retention, false);
    END IF;
END;
$$;


-- 5. Stale servers cleanup (orphaned or no active deployments, 90 days)
CREATE OR REPLACE FUNCTION stacker_cleanup_stale_servers(
    retention INTERVAL DEFAULT INTERVAL '90 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM server s
        WHERE NOT EXISTS (SELECT 1 FROM project p WHERE p.id = s.project_id)
           OR (
               s.updated_at < NOW() - retention
               AND NOT EXISTS (
                   SELECT 1 FROM deployment d
                   WHERE d.project_id = s.project_id AND (d.deleted = false OR d.deleted IS NULL)
               )
           );
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_stale_servers', 'server', v_count, retention, true);
    ELSE
        DELETE FROM server s
        WHERE NOT EXISTS (SELECT 1 FROM project p WHERE p.id = s.project_id)
           OR (
               s.updated_at < NOW() - retention
               AND NOT EXISTS (
                   SELECT 1 FROM deployment d
                   WHERE d.project_id = s.project_id AND (d.deleted = false OR d.deleted IS NULL)
               )
           );
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_stale_servers', 'server', v_count, retention, false);
    END IF;
END;
$$;


-- 6. Stale clouds cleanup (no servers referencing them, 90 days)
CREATE OR REPLACE FUNCTION stacker_cleanup_stale_clouds(
    retention INTERVAL DEFAULT INTERVAL '90 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM cloud c
        WHERE c.updated_at < NOW() - retention
          AND NOT EXISTS (SELECT 1 FROM server s WHERE s.cloud_id = c.id);
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_stale_clouds', 'cloud', v_count, retention, true);
    ELSE
        DELETE FROM cloud c
        WHERE c.updated_at < NOW() - retention
          AND NOT EXISTS (SELECT 1 FROM server s WHERE s.cloud_id = c.id);
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_stale_clouds', 'cloud', v_count, retention, false);
    END IF;
END;
$$;


-- 7. Event tables retention (180 days)
CREATE OR REPLACE FUNCTION stacker_cleanup_events(
    retention INTERVAL DEFAULT INTERVAL '180 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM marketplace_template_event WHERE occurred_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_events', 'marketplace_template_event', v_count, retention, true);

        SELECT count(*) INTO v_count FROM marketplace_event WHERE occurred_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_events', 'marketplace_event', v_count, retention, true);

        SELECT count(*) INTO v_count FROM stack_template_deployment WHERE created_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_events', 'stack_template_deployment', v_count, retention, true);
    ELSE
        DELETE FROM marketplace_template_event WHERE occurred_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_events', 'marketplace_template_event', v_count, retention, false);

        DELETE FROM marketplace_event WHERE occurred_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_events', 'marketplace_event', v_count, retention, false);

        DELETE FROM stack_template_deployment WHERE created_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_events', 'stack_template_deployment', v_count, retention, false);
    END IF;
END;
$$;


-- Schedule pg_cron jobs (staggered to avoid load spikes)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        -- Audit logs: daily 4:00 AM
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_audit_logs') THEN
            PERFORM cron.schedule('stacker_cleanup_audit_logs', '0 4 * * *',
                $cron$SELECT stacker_cleanup_audit_logs(p_dry_run := false);$cron$);
        END IF;

        -- Terminal commands: daily 4:15 AM
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_terminal_commands') THEN
            PERFORM cron.schedule('stacker_cleanup_terminal_commands', '15 4 * * *',
                $cron$SELECT stacker_cleanup_terminal_commands(p_dry_run := false);$cron$);
        END IF;

        -- Deleted deployments: daily 4:30 AM
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_deleted_deployments') THEN
            PERFORM cron.schedule('stacker_cleanup_deleted_deployments', '30 4 * * *',
                $cron$SELECT stacker_cleanup_deleted_deployments(p_dry_run := false);$cron$);
        END IF;

        -- Stale projects: daily 4:40 AM
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_stale_projects') THEN
            PERFORM cron.schedule('stacker_cleanup_stale_projects', '40 4 * * *',
                $cron$SELECT stacker_cleanup_stale_projects(p_dry_run := false);$cron$);
        END IF;

        -- Stale servers: daily 4:45 AM
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_stale_servers') THEN
            PERFORM cron.schedule('stacker_cleanup_stale_servers', '45 4 * * *',
                $cron$SELECT stacker_cleanup_stale_servers(p_dry_run := false);$cron$);
        END IF;

        -- Stale clouds: daily 4:50 AM
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_stale_clouds') THEN
            PERFORM cron.schedule('stacker_cleanup_stale_clouds', '50 4 * * *',
                $cron$SELECT stacker_cleanup_stale_clouds(p_dry_run := false);$cron$);
        END IF;

        -- Events: weekly Sunday 5:00 AM
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_events') THEN
            PERFORM cron.schedule('stacker_cleanup_events', '0 5 * * 0',
                $cron$SELECT stacker_cleanup_events(p_dry_run := false);$cron$);
        END IF;
    END IF;
END;
$$;
