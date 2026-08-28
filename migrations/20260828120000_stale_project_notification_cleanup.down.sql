-- Rollback: remove stale project notification cleanup

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        PERFORM cron.unschedule('stacker_mark_stale_projects');
        PERFORM cron.unschedule('stacker_cleanup_stale_deployments');
        PERFORM cron.unschedule('stacker_cleanup_expired_projects');

        -- Restore the old stale_projects cron
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_stale_projects') THEN
            PERFORM cron.schedule('stacker_cleanup_stale_projects', '40 4 * * *',
                $cron$SELECT stacker_cleanup_stale_projects(p_dry_run := false);$cron$);
        END IF;
    END IF;
END;
$$;

DROP FUNCTION IF EXISTS stacker_mark_stale_projects(INTERVAL, INTERVAL, BOOLEAN);
DROP FUNCTION IF EXISTS stacker_cleanup_expired_projects(BOOLEAN);
DROP FUNCTION IF EXISTS stacker_cleanup_stale_deployments(INTERVAL, BOOLEAN);

-- Restore original stacker_cleanup_stale_projects (without deletion_scheduled_at filter)
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

-- Remove the columns
DROP INDEX IF EXISTS idx_project_deletion_scheduled;
ALTER TABLE project DROP COLUMN IF EXISTS deletion_scheduled_at;
ALTER TABLE project DROP COLUMN IF EXISTS deletion_warning_sent_at;
