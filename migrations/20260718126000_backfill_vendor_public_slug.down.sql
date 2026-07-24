-- Revert: clear auto-generated slugs.
-- Only clears slugs that look like the auto-generated format (no custom slugs).
-- Custom slugs set via the API won't match the creator_user_id pattern.
UPDATE marketplace_vendor_profile
SET public_slug = NULL
WHERE public_slug = LEFT(
    REGEXP_REPLACE(LOWER(creator_user_id), '[^a-z0-9]+', '-', 'g'),
    100
);
