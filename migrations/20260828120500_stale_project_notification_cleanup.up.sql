-- Stale project notification cleanup: mark → notify → grace period → auto-delete.
-- Extends the data retention cleanup (migration 20260731120000) with a two-phase
-- deletion workflow that warns users before removing abandoned projects.
--
-- Workflow:
--   1. stacker_mark_stale_projects: marks projects for deletion + sends notifications
--   2. 30-day grace period: any project update resets deletion_scheduled_at to NULL
--   3. stacker_cleanup_expired_projects: deletes projects past the grace period
--
-- Also adds stacker_cleanup_stale_deployments to catch orphaned deployments
-- that block project cleanup.


-- 1. Add deletion tracking columns to project table
ALTER TABLE project ADD COLUMN IF NOT EXISTS deletion_scheduled_at TIMESTAMPTZ;
ALTER TABLE project ADD COLUMN IF NOT EXISTS deletion_warning_sent_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_project_deletion_scheduled
    ON project(deletion_scheduled_at)
    WHERE deletion_scheduled_at IS NOT NULL;


-- 2. Mark stale projects for deletion
-- Finds projects with no active deployments, no servers, not protected,
-- updated_at older than stale_threshold, and not already marked.
-- Sets deletion_scheduled_at and returns the marked projects for notification.
CREATE OR REPLACE FUNCTION stacker_mark_stale_projects(
    stale_threshold INTERVAL DEFAULT INTERVAL '90 days',
    grace_period INTERVAL DEFAULT INTERVAL '30 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS TABLE(project_id INT, project_name TEXT, project_user_id TEXT)
LANGUAGE plpgsql AS $$
DECLARE
    v_project RECORD;
    v_count BIGINT := 0;
BEGIN
    FOR v_project IN
        SELECT p.id, p.name, p.user_id
        FROM project p
        WHERE p.is_protected = false
          AND p.deletion_scheduled_at IS NULL
          AND p.updated_at < NOW() - stale_threshold
          AND NOT EXISTS (
              SELECT 1 FROM deployment d
              WHERE d.project_id = p.id AND (d.deleted = false OR d.deleted IS NULL)
          )
          AND NOT EXISTS (SELECT 1 FROM server s WHERE s.project_id = p.id)
    LOOP
        IF p_dry_run THEN
            INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
            VALUES ('stacker_mark_stale_projects', 'project', 1, stale_threshold, true);
        ELSE
            UPDATE project
            SET deletion_scheduled_at = NOW() + grace_period
            WHERE id = v_project.id;
        END IF;

        v_count := v_count + 1;
        project_id := v_project.id;
        project_name := v_project.name;
        project_user_id := v_project.user_id;
        RETURN NEXT;
    END LOOP;

    IF NOT p_dry_run THEN
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_mark_stale_projects', 'project', v_count, stale_threshold, false);
    END IF;
END;
$$;


-- 3. Cleanup expired projects (past grace period)
-- Deletes projects where deletion_scheduled_at has passed AND the project
-- is still stale (no active deployments, no servers).
-- Also resets deletion_scheduled_at on projects that became active again.
CREATE OR REPLACE FUNCTION stacker_cleanup_expired_projects(
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    -- First, reset deletion_scheduled_at on projects that are no longer stale
    -- (gained active deployments or servers, or were updated recently)
    IF NOT p_dry_run THEN
        UPDATE project
        SET deletion_scheduled_at = NULL
        WHERE deletion_scheduled_at IS NOT NULL
          AND (
              updated_at > deletion_scheduled_at - INTERVAL '30 days'
              OR EXISTS (
                  SELECT 1 FROM deployment d
                  WHERE d.project_id = project.id AND (d.deleted = false OR d.deleted IS NULL)
              )
              OR EXISTS (
                  SELECT 1 FROM server s WHERE s.project_id = project.id
              )
              OR is_protected = true
          );
    END IF;

    -- Then delete projects that are past the grace period and still stale
    IF p_dry_run THEN
        SELECT count(*) INTO v_count
        FROM project p
        WHERE p.deletion_scheduled_at IS NOT NULL
          AND p.deletion_scheduled_at < NOW()
          AND p.is_protected = false
          AND NOT EXISTS (
              SELECT 1 FROM deployment d
              WHERE d.project_id = p.id AND (d.deleted = false OR d.deleted IS NULL)
          )
          AND NOT EXISTS (SELECT 1 FROM server s WHERE s.project_id = p.id);
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_expired_projects', 'project', v_count, NULL, true);
    ELSE
        DELETE FROM project p
        WHERE p.deletion_scheduled_at IS NOT NULL
          AND p.deletion_scheduled_at < NOW()
          AND p.is_protected = false
          AND NOT EXISTS (
              SELECT 1 FROM deployment d
              WHERE d.project_id = p.id AND (d.deleted = false OR d.deleted IS NULL)
          )
          AND NOT EXISTS (SELECT 1 FROM server s WHERE s.project_id = p.id);
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_expired_projects', 'project', v_count, NULL, false);
    END IF;
END;
$$;


-- 4. Update stacker_cleanup_stale_projects to skip already-marked projects
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
          AND p.deletion_scheduled_at IS NULL
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
          AND p.deletion_scheduled_at IS NULL
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


-- 5. Cleanup stale deployments (orphaned: no server for the project, old updated_at)
-- This unblocks project cleanup for projects that have abandoned deployments.
CREATE OR REPLACE FUNCTION stacker_cleanup_stale_deployments(
    retention INTERVAL DEFAULT INTERVAL '90 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM deployment d
        WHERE (d.deleted = false OR d.deleted IS NULL)
          AND d.updated_at < NOW() - retention
          AND NOT EXISTS (
              SELECT 1 FROM server s WHERE s.project_id = d.project_id
          );
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_stale_deployments', 'deployment', v_count, retention, true);
    ELSE
        DELETE FROM deployment d
        WHERE (d.deleted = false OR d.deleted IS NULL)
          AND d.updated_at < NOW() - retention
          AND NOT EXISTS (
              SELECT 1 FROM server s WHERE s.project_id = d.project_id
          );
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_stale_deployments', 'deployment', v_count, retention, false);
    END IF;
END;
$$;


-- 6. Schedule pg_cron jobs
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_cron') THEN
        -- Mark stale projects (notify users): daily 3:50 AM
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_mark_stale_projects') THEN
            PERFORM cron.schedule('stacker_mark_stale_projects', '50 3 * * *',
                $cron$SELECT stacker_mark_stale_projects(p_dry_run := false);$cron$);
        END IF;

        -- Stale deployments cleanup: daily 4:35 AM (before project cleanup)
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_stale_deployments') THEN
            PERFORM cron.schedule('stacker_cleanup_stale_deployments', '35 4 * * *',
                $cron$SELECT stacker_cleanup_stale_deployments(p_dry_run := false);$cron$);
        END IF;

        -- Expired projects cleanup: daily 4:40 AM
        IF NOT EXISTS (SELECT 1 FROM cron.job WHERE jobname = 'stacker_cleanup_expired_projects') THEN
            PERFORM cron.schedule('stacker_cleanup_expired_projects', '40 4 * * *',
                $cron$SELECT stacker_cleanup_expired_projects(p_dry_run := false);$cron$);
        END IF;

        -- Remove the old stale_projects cron (replaced by mark + expire)
        PERFORM cron.unschedule('stacker_cleanup_stale_projects');
    END IF;
END;
$$;
