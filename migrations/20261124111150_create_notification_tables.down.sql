DROP INDEX IF EXISTS idx_notification_dispatch_event;
DROP INDEX IF EXISTS idx_notification_dispatch_status;
DROP INDEX IF EXISTS idx_notification_dispatch_channel;
DROP TABLE IF EXISTS notification_dispatch;

DROP INDEX IF EXISTS idx_notification_channel_enabled;
DROP INDEX IF EXISTS idx_notification_channel_project;
DROP INDEX IF EXISTS idx_notification_channel_global_unique;
DROP INDEX IF EXISTS idx_notification_channel_project_unique;
DROP TABLE IF EXISTS notification_channel;
