ALTER TABLE marketplace_vendor_profile
ADD COLUMN IF NOT EXISTS public_slug VARCHAR(100),
ADD COLUMN IF NOT EXISTS display_name VARCHAR(255),
ADD COLUMN IF NOT EXISTS bio TEXT,
ADD COLUMN IF NOT EXISTS avatar_url TEXT,
ADD COLUMN IF NOT EXISTS website_url TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS marketplace_vendor_profile_public_slug_unique
ON marketplace_vendor_profile (public_slug)
WHERE public_slug IS NOT NULL;
