-- Revert server cleanup notification changes

DROP TRIGGER IF EXISTS trg_reset_server_deletion ON server;
DROP FUNCTION IF EXISTS reset_server_deletion_flags();

SELECT cron.unschedule('stacker_mark_stale_servers');
SELECT cron.unschedule('stacker_cleanup_expired_servers');

DROP FUNCTION IF EXISTS stacker_cleanup_expired_servers(BOOLEAN);
DROP FUNCTION IF EXISTS stacker_mark_stale_servers(INTERVAL, INTERVAL, BOOLEAN);

DROP INDEX IF EXISTS idx_server_deletion_scheduled;
ALTER TABLE server DROP COLUMN IF EXISTS deletion_warning_sent_at;
ALTER TABLE server DROP COLUMN IF EXISTS deletion_scheduled_at;
