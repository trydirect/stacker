-- Add up migration script here
ALTER table server ADD COLUMN project_id integer CONSTRAINT project_id REFERENCES project(id) ON UPDATE CASCADE ON DELETE CASCADE;
