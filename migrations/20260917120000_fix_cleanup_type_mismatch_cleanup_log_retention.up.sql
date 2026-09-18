-- Fix stacker_cleanup_terminal_commands type mismatch and add cleanup_log retention

-- 1. Fix stacker_cleanup_terminal_commands: command_queue.command_id is VARCHAR
--    referencing commands.command_id (VARCHAR), NOT commands.id (UUID).
--    The original function used `SELECT id FROM commands` which caused:
--    ERROR: operator does not exist: character varying = uuid
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
        -- Use command_id (VARCHAR) not id (UUID) to match command_queue.command_id type
        DELETE FROM command_queue WHERE command_id IN (
            SELECT command_id FROM commands
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


-- 2. Add cleanup_log self-cleanup (90 days retention)
--    cleanup_log accumulates entries from all 7 cleanup functions daily but has no purge.
CREATE OR REPLACE FUNCTION stacker_cleanup_cleanup_log(
    retention INTERVAL DEFAULT INTERVAL '90 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM cleanup_log WHERE run_at < NOW() - retention;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_cleanup_log', 'cleanup_log', v_count, retention, true);
    ELSE
        DELETE FROM cleanup_log WHERE run_at < NOW() - retention;
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_cleanup_log', 'cleanup_log', v_count, retention, false);
    END IF;
END;
$$;


-- 3. Schedule cleanup_log purge (weekly Sunday 5:30 AM)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_cleanup_log') THEN
            PERFORM cron.schedule('stacker_cleanup_cleanup_log', '30 5 * * 0',
                $cron$SELECT stacker_cleanup_cleanup_log(p_dry_run := false);$cron$);
        END IF;
    END IF;
END;
$$;
