-- Deployment daily billing model
-- Adds daily_rate/monthly_cap to stack_template and extends
-- marketplace_install_authorization with daily billing tracking.

-- stack_template: daily billing fields
ALTER TABLE stack_template
  ADD COLUMN IF NOT EXISTS daily_rate DECIMAL(10,2) DEFAULT NULL,
  ADD COLUMN IF NOT EXISTS monthly_cap DECIMAL(10,2) DEFAULT NULL;

-- marketplace_install_authorization: daily billing tracking
ALTER TABLE marketplace_install_authorization
  ADD COLUMN IF NOT EXISTS daily_rate DECIMAL(10,2) DEFAULT 0,
  ADD COLUMN IF NOT EXISTS monthly_cap DECIMAL(10,2) DEFAULT 0,
  ADD COLUMN IF NOT EXISTS total_charged_minor BIGINT DEFAULT 0,
  ADD COLUMN IF NOT EXISTS last_daily_charge_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS server_deleted_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS suspended_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS billing_cycle VARCHAR(50) DEFAULT 'per_install';

-- Platform default daily rates per server type
CREATE TABLE IF NOT EXISTS server_type_daily_rate (
  server_type VARCHAR(50) PRIMARY KEY,
  daily_rate DECIMAL(10,2) NOT NULL,
  monthly_cap DECIMAL(10,2) NOT NULL,
  hetzner_monthly_eur DECIMAL(10,2),
  created_at TIMESTAMPTZ DEFAULT NOW(),
  updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Seed default pricing (Hetzner EUR × 1.1 exchange × 1.5 margin / 30 days)
INSERT INTO server_type_daily_rate (server_type, daily_rate, monthly_cap, hetzner_monthly_eur)
VALUES
  ('cpx11', 0.27, 8.00, 4.85),
  ('cpx22', 0.47, 14.00, 8.55),
  ('cpx32', 0.86, 26.00, 15.59),
  ('cpx42', 1.67, 50.00, 30.39),
  ('cpx52', 3.30, 99.00, 59.99)
ON CONFLICT (server_type) DO NOTHING;

-- Index for sweeper queries
CREATE INDEX IF NOT EXISTS idx_authorization_billing_cycle
  ON marketplace_install_authorization (billing_cycle, status);

CREATE INDEX IF NOT EXISTS idx_authorization_daily_sweep
  ON marketplace_install_authorization (billing_cycle, status, last_daily_charge_at)
  WHERE billing_cycle = 'deployment_daily' AND status = 'captured';
