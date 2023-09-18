-- Add up migration script here

CREATE TABLE product (
    id integer NOT NULL, PRIMARY KEY(id),
    obj_id integer NOT NULL,
    obj_type TEXT NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

CREATE TABLE rating (
    id serial,
    user_id uuid NOT NULL,
    product_id integer NOT NULL,
    category VARCHAR(255) NOT NULL,
    comment TEXT DEFAULT NULL,
    hidden BOOLEAN DEFAULT FALSE,
    rate INTEGER,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    CONSTRAINT fk_product FOREIGN KEY(product_id) REFERENCES product(id),
    CONSTRAINT rating_pk PRIMARY KEY (id)
);

CREATE INDEX idx_category ON rating(category);
CREATE INDEX idx_user_id ON rating(user_id);
CREATE INDEX idx_product_id_rating_id ON rating(product_id, rate);
