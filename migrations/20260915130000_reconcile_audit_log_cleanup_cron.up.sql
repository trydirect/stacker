-- Reconcile the audit-log cleanup job after pg_cron becomes available.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        IF EXISTS (
            SELECT 1 FROM pg_proc
            WHERE proname = 'stacker_cleanup_audit_logs'
        ) THEN
            IF NOT EXISTS (
                SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_audit_logs'
            ) THEN
                PERFORM cron.schedule(
                    'stacker_cleanup_audit_logs',
                    '0 4 * * *',
                    $cron$SELECT stacker_cleanup_audit_logs(p_dry_run := false);$cron$
                );
            END IF;
        END IF;
    END IF;
END;
$$;
