-- Backfill public_slug for existing vendor profiles that don't have one.
-- Derives a slug from creator_user_id: lowercase, replace non-alphanumeric
-- chars with hyphens, strip leading/trailing hyphens, cap at 100 chars.
-- Vendors can change this via PATCH /api/templates/mine/vendor-profile.
UPDATE marketplace_vendor_profile
SET public_slug = LEFT(
    REGEXP_REPLACE(
        LOWER(creator_user_id),
        '[^a-z0-9]+', '-', 'g'
    ),
    100
)
WHERE public_slug IS NULL;
