-- Add migration script here
CREATE TABLE user_stack (
    id uuid NOT NULL, PRIMARY KEY(id),
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    body JSON NOT NULL,
    created_at timestamptz NOT NULL
    updated_at timestamptz NOT NULL
)