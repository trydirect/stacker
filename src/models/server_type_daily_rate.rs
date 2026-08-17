use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use sqlx::FromRow;

/// Platform default daily billing rates per Hetzner server type.
/// Stored in the `server_type_daily_rate` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct ServerTypeDailyRate {
    pub server_type: String,
    pub daily_rate: f64,
    pub monthly_cap: f64,
    pub hetzner_monthly_eur: Option<f64>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl ServerTypeDailyRate {
    /// Calculate daily rate from Hetzner monthly EUR cost.
    /// Formula: hetzner_eur × 1.1 (exchange) × 1.5 (margin) / 30 days
    pub fn calculate_daily_rate(hetzner_monthly_eur: f64) -> f64 {
        let daily = hetzner_monthly_eur * 1.1 * 1.5 / 30.0;
        (daily * 100.0).round() / 100.0 // round to 2 decimal places
    }

    /// Calculate monthly cap from daily rate (30 days).
    pub fn calculate_monthly_cap(daily_rate: f64) -> f64 {
        (daily_rate * 30.0 * 100.0).round() / 100.0
    }
}
