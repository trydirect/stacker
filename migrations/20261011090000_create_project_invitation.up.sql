-- Invitations to join a project as a member or viewer (Team seats).
--
-- A pending invitation holds a seat until it is accepted, revoked or expires.
-- Only the hash of the invitation token is stored; the token itself goes to
-- the invited address in the email link.
CREATE TABLE IF NOT EXISTS project_invitation (
    id UUID PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    role VARCHAR(32) NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    invited_by TEXT NOT NULL,
    invited_by_email TEXT NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_by TEXT NULL,
    accepted_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT project_invitation_email_lower CHECK (email = lower(email)),
    CONSTRAINT project_invitation_status CHECK (status IN ('pending', 'accepted', 'revoked', 'expired'))
);

-- One live invitation per address per project.
CREATE UNIQUE INDEX IF NOT EXISTS idx_project_invitation_pending_email
    ON project_invitation (project_id, email)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_project_invitation_email_pending
    ON project_invitation (email)
    WHERE status = 'pending';
