CREATE TABLE user_stack (
    id serial4 NOT NULL,
    stack_id uuid NOT NULL,
    user_id VARCHAR(50) NOT NULL,
    name TEXT NOT NULL UNIQUE,
    body JSON NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    CONSTRAINT user_stack_pkey PRIMARY KEY (id)
);

CREATE INDEX idx_stack_id ON user_stack(stack_id);
CREATE INDEX idx_stack_user_id ON user_stack(user_id);