-- Revert: restore original stacker_cleanup_terminal_commands, drop cleanup_log cleanup

-- Unschedule cleanup_log cron job
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        PERFORM cron.unschedule('stacker_cleanup_cleanup_log');
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END;
$$;

-- Drop cleanup_log cleanup function
DROP FUNCTION IF EXISTS stacker_cleanup_cleanup_log(INTERVAL, BOOLEAN);

-- Restore original stacker_cleanup_terminal_commands (with the type mismatch bug)
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
