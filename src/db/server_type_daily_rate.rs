use sqlx::PgPool;

use crate::models::ServerTypeDailyRate;

/// Fetch daily rate configuration for a server type.
pub async fn fetch(
    pool: &PgPool,
    server_type: &str,
) -> Result<Option<ServerTypeDailyRate>, String> {
    sqlx::query_as::<_, ServerTypeDailyRate>(
        r#"SELECT server_type, daily_rate, monthly_cap, hetzner_monthly_eur,
                  created_at, updated_at
           FROM server_type_daily_rate
           WHERE server_type = $1"#,
    )
    .bind(server_type)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch server_type_daily_rate: {}", e))
}

/// List all server type daily rate configurations.
pub async fn list(pool: &PgPool) -> Result<Vec<ServerTypeDailyRate>, String> {
    sqlx::query_as::<_, ServerTypeDailyRate>(
        r#"SELECT server_type, daily_rate, monthly_cap, hetzner_monthly_eur,
                  created_at, updated_at
           FROM server_type_daily_rate
           ORDER BY daily_rate ASC"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list server_type_daily_rate: {}", e))
}

/// Upsert daily rate for a server type.
pub async fn upsert(
    pool: &PgPool,
    server_type: &str,
    daily_rate: f64,
    monthly_cap: f64,
    hetzner_monthly_eur: Option<f64>,
) -> Result<ServerTypeDailyRate, String> {
    sqlx::query_as::<_, ServerTypeDailyRate>(
        r#"INSERT INTO server_type_daily_rate (server_type, daily_rate, monthly_cap, hetzner_monthly_eur, updated_at)
           VALUES ($1, $2, $3, $4, NOW())
           ON CONFLICT (server_type) DO UPDATE SET
             daily_rate = EXCLUDED.daily_rate,
             monthly_cap = EXCLUDED.monthly_cap,
             hetzner_monthly_eur = EXCLUDED.hetzner_monthly_eur,
             updated_at = NOW()
           RETURNING server_type, daily_rate, monthly_cap, hetzner_monthly_eur, created_at, updated_at"#,
    )
    .bind(server_type)
    .bind(daily_rate)
    .bind(monthly_cap)
    .bind(hetzner_monthly_eur)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to upsert server_type_daily_rate: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_daily_rate_formula() {
        // cpx11: €4.85/mo → $0.27/day
        let rate = ServerTypeDailyRate::calculate_daily_rate(4.85);
        assert!((rate - 0.27).abs() < 0.01, "expected ~0.27, got {}", rate);

        // cpx32: €15.59/mo → $0.86/day
        let rate = ServerTypeDailyRate::calculate_daily_rate(15.59);
        assert!((rate - 0.86).abs() < 0.01, "expected ~0.86, got {}", rate);

        // cpx42: €30.39/mo → $1.67/day
        let rate = ServerTypeDailyRate::calculate_daily_rate(30.39);
        assert!((rate - 1.67).abs() < 0.01, "expected ~1.67, got {}", rate);
    }

    #[test]
    fn calculate_monthly_cap() {
        let cap = ServerTypeDailyRate::calculate_monthly_cap(0.86);
        assert!((cap - 25.80).abs() < 0.01, "expected ~25.80, got {}", cap);

        let cap = ServerTypeDailyRate::calculate_monthly_cap(1.67);
        assert!((cap - 50.10).abs() < 0.01, "expected ~50.10, got {}", cap);
    }
}
