-- Chat sessions: multiple AI chat dialogs per user for the Stack Builder page.
-- Each session owns its message history as an AES-256-GCM encrypted blob
-- (same `Secret` helper used for cloud credentials). Legacy single-blob
-- `chat_conversations` is left untouched.
CREATE TABLE chat_sessions (
    id                 UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id            VARCHAR(255) NOT NULL,               -- owner; all authz scopes on this
    project_id         INTEGER,                             -- NULL = canvas / onboarding mode
    title              VARCHAR(255),                        -- nullable, auto-title later
    messages_encrypted TEXT         NOT NULL DEFAULT '',    -- base64(nonce || AES-256-GCM ciphertext); '' = empty session
    created_at         TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- List query orders a user's sessions newest-first, optionally filtered by project.
CREATE INDEX idx_chat_sessions_user_project_updated
    ON chat_sessions (user_id, project_id, updated_at DESC);
