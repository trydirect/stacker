-- Ensure the audit-log retention job is installed on databases where the
-- retention migration ran before pg_cron was available.
DO $$
BEGIN
    CREATE EXTENSION IF NOT EXISTS pg_cron;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'pg_cron is unavailable; audit-log cleanup was not scheduled: %', SQLERRM;
END;
$$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron')
       AND EXISTS (
           SELECT 1 FROM pg_proc
           WHERE proname = 'stacker_cleanup_audit_logs'
       )
       AND NOT EXISTS (
           SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_audit_logs'
       ) THEN
        PERFORM cron.schedule(
            'stacker_cleanup_audit_logs',
            '0 4 * * *',
            $cron$SELECT stacker_cleanup_audit_logs(p_dry_run := false);$cron$
        );
    END IF;
END;
$$;
