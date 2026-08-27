DROP INDEX IF EXISTS idx_chat_sessions_user_active;
ALTER TABLE chat_sessions DROP COLUMN IF EXISTS archived_at;
