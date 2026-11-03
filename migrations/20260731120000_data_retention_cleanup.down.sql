-- Remove all data retention cleanup functions and schedules

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        PERFORM cron.unschedule('stacker_cleanup_audit_logs');
        PERFORM cron.unschedule('stacker_cleanup_terminal_commands');
        PERFORM cron.unschedule('stacker_cleanup_deleted_deployments');
        PERFORM cron.unschedule('stacker_cleanup_stale_projects');
        PERFORM cron.unschedule('stacker_cleanup_stale_servers');
        PERFORM cron.unschedule('stacker_cleanup_stale_clouds');
        PERFORM cron.unschedule('stacker_cleanup_events');
    END IF;
END;
$$;

DROP FUNCTION IF EXISTS stacker_cleanup_audit_logs(INTERVAL, BOOLEAN);
DROP FUNCTION IF EXISTS stacker_cleanup_terminal_commands(INTERVAL, BOOLEAN);
DROP FUNCTION IF EXISTS stacker_cleanup_deleted_deployments(INTERVAL, BOOLEAN);
DROP FUNCTION IF EXISTS stacker_cleanup_stale_projects(INTERVAL, BOOLEAN);
DROP FUNCTION IF EXISTS stacker_cleanup_stale_servers(INTERVAL, BOOLEAN);
DROP FUNCTION IF EXISTS stacker_cleanup_stale_clouds(INTERVAL, BOOLEAN);
DROP FUNCTION IF EXISTS stacker_cleanup_events(INTERVAL, BOOLEAN);
DROP TABLE IF EXISTS cleanup_log;
