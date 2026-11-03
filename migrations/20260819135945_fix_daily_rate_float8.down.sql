-- Revert daily_rate/monthly_cap columns back to NUMERIC
-- (should only be run if the code is also reverted to bind Decimal)
ALTER TABLE stack_template
  ALTER COLUMN daily_rate TYPE DECIMAL(10,2) USING daily_rate::numeric(10,2),
  ALTER COLUMN monthly_cap TYPE DECIMAL(10,2) USING monthly_cap::numeric(10,2);

ALTER TABLE marketplace_install_authorization
  ALTER COLUMN daily_rate TYPE DECIMAL(10,2) USING daily_rate::numeric(10,2),
  ALTER COLUMN monthly_cap TYPE DECIMAL(10,2) USING monthly_cap::numeric(10,2);

ALTER TABLE server_type_daily_rate
  ALTER COLUMN daily_rate TYPE DECIMAL(10,2) USING daily_rate::numeric(10,2),
  ALTER COLUMN monthly_cap TYPE DECIMAL(10,2) USING monthly_cap::numeric(10,2),
  ALTER COLUMN hetzner_monthly_eur TYPE DECIMAL(10,2) USING hetzner_monthly_eur::numeric(10,2);