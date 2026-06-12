DROP INDEX IF EXISTS marketplace_vendor_profile_public_slug_unique;

ALTER TABLE marketplace_vendor_profile
DROP COLUMN IF EXISTS website_url,
DROP COLUMN IF EXISTS avatar_url,
DROP COLUMN IF EXISTS bio,
DROP COLUMN IF EXISTS display_name,
DROP COLUMN IF EXISTS public_slug;
