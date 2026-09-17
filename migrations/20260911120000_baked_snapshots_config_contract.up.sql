-- Pin the author-declared field policy (config_contract) to the baked image.
-- The clone path reads this to regenerate `mutability: generated` fields fresh
-- per buyer, instead of every clone inheriting the one value baked at bake time.
ALTER TABLE baked_snapshots ADD COLUMN config_contract JSONB;
