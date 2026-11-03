-- Server cleanup notification: mark -> notify -> grace period -> auto-delete.
-- Follows the same pattern as project cleanup (migration 20260828120500).
--
-- Workflow:
--   1. stacker_mark_stale_servers: marks servers for deletion (90 days inactive)
--   2. cleanup-notify binary sends email + bell notification
--   3. 30-day grace period: any server update resets deletion_scheduled_at
--   4. stacker_cleanup_expired_servers: deletes servers past grace period


-- 1. Add deletion tracking columns to server table
ALTER TABLE server ADD COLUMN IF NOT EXISTS deletion_scheduled_at TIMESTAMPTZ;
ALTER TABLE server ADD COLUMN IF NOT EXISTS deletion_warning_sent_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_server_deletion_scheduled
    ON server(deletion_scheduled_at)
    WHERE deletion_scheduled_at IS NOT NULL;


-- 2. Mark stale servers for deletion
-- A server is stale if:
--   - updated_at is older than stale_threshold (default 90 days)
--   - It has no active deployments (no deployment with project_id = server.project_id)
--   - It is not already marked (deletion_scheduled_at IS NULL)
-- Sets deletion_scheduled_at = NOW() + grace_period (default 30 days).
CREATE OR REPLACE FUNCTION stacker_mark_stale_servers(
    stale_threshold INTERVAL DEFAULT INTERVAL '90 days',
    grace_period INTERVAL DEFAULT INTERVAL '30 days',
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS TABLE(server_id INT, server_name TEXT, server_user_id TEXT, server_ip TEXT)
LANGUAGE plpgsql AS $$
DECLARE
    v_server RECORD;
    v_count BIGINT := 0;
BEGIN
    FOR v_server IN
        SELECT s.id, s.name, s.user_id, s.srv_ip
        FROM server s
        WHERE s.deletion_scheduled_at IS NULL
          AND s.updated_at < NOW() - stale_threshold
          AND NOT EXISTS (
              SELECT 1 FROM deployment d
              WHERE d.project_id = s.project_id
                AND (d.deleted = false OR d.deleted IS NULL)
          )
    LOOP
        IF p_dry_run THEN
            INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
            VALUES ('stacker_mark_stale_servers', 'server', 1, stale_threshold, true);
        ELSE
            UPDATE server
            SET deletion_scheduled_at = NOW() + grace_period
            WHERE id = v_server.id;
        END IF;

        v_count := v_count + 1;
        server_id := v_server.id;
        server_name := COALESCE(v_server.name, 'unnamed');
        server_user_id := v_server.user_id;
        server_ip := v_server.srv_ip;
        RETURN NEXT;
    END LOOP;

    IF NOT p_dry_run THEN
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_mark_stale_servers', 'server', v_count, stale_threshold, false);
    END IF;
END;
$$;


-- 3. Cleanup expired servers (past grace period)
-- Deletes servers where deletion_scheduled_at has passed AND the server
-- is still stale (no active deployments, not updated recently).
-- Resets deletion_scheduled_at on servers that became active again.
CREATE OR REPLACE FUNCTION stacker_cleanup_expired_servers(
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_count BIGINT;
BEGIN
    -- Reset deletion_scheduled_at on servers that became active again
    IF NOT p_dry_run THEN
        UPDATE server
        SET deletion_scheduled_at = NULL,
            deletion_warning_sent_at = NULL
        WHERE deletion_scheduled_at IS NOT NULL
          AND (
              updated_at > deletion_scheduled_at - INTERVAL '30 days'
              OR EXISTS (
                  SELECT 1 FROM deployment d
                  WHERE d.project_id = server.project_id
                    AND (d.deleted = false OR d.deleted IS NULL)
              )
          );
    END IF;

    -- Delete expired servers past grace period
    IF p_dry_run THEN
        SELECT count(*) INTO v_count FROM server s
        WHERE s.deletion_scheduled_at IS NOT NULL
          AND s.deletion_scheduled_at < NOW()
          AND NOT EXISTS (
              SELECT 1 FROM deployment d
              WHERE d.project_id = s.project_id
                AND (d.deleted = false OR d.deleted IS NULL)
          );
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_expired_servers', 'server', v_count, INTERVAL '0', true);
    ELSE
        DELETE FROM server s
        WHERE s.deletion_scheduled_at IS NOT NULL
          AND s.deletion_scheduled_at < NOW()
          AND NOT EXISTS (
              SELECT 1 FROM deployment d
              WHERE d.project_id = s.project_id
                AND (d.deleted = false OR d.deleted IS NULL)
          );
        GET DIAGNOSTICS v_count = ROW_COUNT;
        INSERT INTO cleanup_log (function_name, table_name, rows_deleted, retention, dry_run)
        VALUES ('stacker_cleanup_expired_servers', 'server', v_count, INTERVAL '0', false);
    END IF;
END;
$$;


-- 4. Schedule pg_cron jobs (wrapped in DO blocks for test/dev environments without pg_cron)
DO $$
BEGIN
    PERFORM cron.schedule(
        'stacker_mark_stale_servers',
        '55 3 * * *',
        'SELECT stacker_mark_stale_servers(INTERVAL ''90 days'', INTERVAL ''30 days'', false)'
    );
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'pg_cron not available, skipping stacker_mark_stale_servers schedule: %', SQLERRM;
END;
$$;

DO $$
BEGIN
    PERFORM cron.schedule(
        'stacker_cleanup_expired_servers',
        '45 4 * * *',
        'SELECT stacker_cleanup_expired_servers(false)'
    );
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'pg_cron not available, skipping stacker_cleanup_expired_servers schedule: %', SQLERRM;
END;
$$;


-- 5. Reset deletion_scheduled_at when a server is updated (any edit = activity)
-- Trigger to clear deletion flags on any server update
CREATE OR REPLACE FUNCTION reset_server_deletion_flags()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.deletion_scheduled_at IS NOT NULL
       AND (OLD.updated_at IS DISTINCT FROM NEW.updated_at
            OR OLD.srv_ip IS DISTINCT FROM NEW.srv_ip
            OR OLD.key_status IS DISTINCT FROM NEW.key_status
            OR OLD.name IS DISTINCT FROM NEW.name)
    THEN
        NEW.deletion_scheduled_at := NULL;
        NEW.deletion_warning_sent_at := NULL;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_reset_server_deletion ON server;
CREATE TRIGGER trg_reset_server_deletion
    BEFORE UPDATE ON server
    FOR EACH ROW
    EXECUTE FUNCTION reset_server_deletion_flags();
