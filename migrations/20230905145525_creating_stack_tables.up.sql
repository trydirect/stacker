-- Add up migration script here
-- Add migration script here
CREATE TABLE user_stack (
    id serial,
    stack_id uuid NOT NULL,
    user_id integer NOT NULL,
    name TEXT NOT NULL UNIQUE,
    body JSON NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
)

