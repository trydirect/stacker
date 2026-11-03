-- Fix daily_rate/monthly_cap columns to FLOAT8 (DOUBLE PRECISION)
-- Rust code binds/decodes these as f64 (sqlx FLOAT8); the original migration
-- created them as DECIMAL(10,2) (NUMERIC), causing:
--   "mismatched types; Rust type Option<f64> (as SQL type FLOAT8)
--    is not compatible with SQL type NUMERIC"
-- Applies to the three tables that carry daily billing values.
ALTER TABLE stack_template
  ALTER COLUMN daily_rate TYPE DOUBLE PRECISION USING daily_rate::double precision,
  ALTER COLUMN monthly_cap TYPE DOUBLE PRECISION USING monthly_cap::double precision;

ALTER TABLE marketplace_install_authorization
  ALTER COLUMN daily_rate TYPE DOUBLE PRECISION USING daily_rate::double precision,
  ALTER COLUMN monthly_cap TYPE DOUBLE PRECISION USING monthly_cap::double precision;

ALTER TABLE server_type_daily_rate
  ALTER COLUMN daily_rate TYPE DOUBLE PRECISION USING daily_rate::double precision,
  ALTER COLUMN monthly_cap TYPE DOUBLE PRECISION USING monthly_cap::double precision,
  ALTER COLUMN hetzner_monthly_eur TYPE DOUBLE PRECISION USING hetzner_monthly_eur::double precision;