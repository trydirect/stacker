-- Add migration script here
CREATE TABLE user_stack (
    id integer NOT NULL, PRIMARY KEY(id),
    stack_id integer NOT NULL,
    user_id integer NOT NULL,
    name TEXT NOT NULL,
    body JSON NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
)

