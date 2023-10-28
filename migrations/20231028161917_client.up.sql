-- Add up migration script here
CREATE TABLE public.client (
	id serial4 NOT NULL,
	user_id varchar(50) NOT NULL,
	"password" varchar(255) NOT NULL,
	CONSTRAINT client_pkey PRIMARY KEY (id)
);
