-- Archive (soft-close) support for chat sessions. NULL = active; a timestamp
-- means the user archived (closed) the thread without deleting it. The "+ new
-- chat" action archives the current session instead of DELETEing it, so the
-- thread stays available in an archived view.
ALTER TABLE chat_sessions ADD COLUMN archived_at TIMESTAMPTZ;

-- Fast default listing of a user's active (non-archived) sessions, newest-first.
CREATE INDEX idx_chat_sessions_user_active
    ON chat_sessions (user_id, updated_at DESC)
    WHERE archived_at IS NULL;
