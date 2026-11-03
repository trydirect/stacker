-- Revert deployment daily billing changes

DROP INDEX IF EXISTS idx_authorization_daily_sweep;
DROP INDEX IF EXISTS idx_authorization_billing_cycle;

DROP TABLE IF EXISTS server_type_daily_rate;

ALTER TABLE marketplace_install_authorization
  DROP COLUMN IF EXISTS daily_rate,
  DROP COLUMN IF EXISTS monthly_cap,
  DROP COLUMN IF EXISTS total_charged_minor,
  DROP COLUMN IF EXISTS last_daily_charge_at,
  DROP COLUMN IF EXISTS server_deleted_at,
  DROP COLUMN IF EXISTS suspended_at,
  DROP COLUMN IF EXISTS billing_cycle;

ALTER TABLE stack_template
  DROP COLUMN IF EXISTS daily_rate,
  DROP COLUMN IF EXISTS monthly_cap;
