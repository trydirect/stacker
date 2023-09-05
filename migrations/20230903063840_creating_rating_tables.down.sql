-- Add down migration script here

DROP INDEX idx_category;
DROP INDEX idx_user_id;
DROP INDEX idx_product_id_rating_id;

DROP table rating;
DROP table product;
