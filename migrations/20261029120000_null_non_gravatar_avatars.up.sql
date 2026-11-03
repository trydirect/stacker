-- Vendor avatars are now derived from the account email via Gravatar
-- (see helpers::gravatar and PATCH /mine/vendor-profile). Stored values
-- from before that change may point at arbitrary external hosts, which
-- must not reach public pages: clear every non-Gravatar avatar_url. The
-- next profile save repopulates the column with the derived Gravatar URL.
UPDATE marketplace_vendor_profile
SET avatar_url = NULL
WHERE avatar_url IS NOT NULL
  AND avatar_url !~* '^https://([a-z0-9-]+\.)?gravatar\.com/';
