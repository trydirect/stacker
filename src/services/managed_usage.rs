//! Daily usage of the Managed server behind a Quick Deploy.
//!
//! Each day a server runs is one Stripe meter event worth that day's rate in
//! cents; the month's events become one invoice, never more than the monthly
//! cap. Contract: config/shared-fixtures/api-contracts/managed-server-billing.json.
//!
//! Pure functions only: the sweeper reads a usage row, asks [`next_report`]
//! for the next day to send, sends it, and stores the returned state.

use chrono::{Datelike, NaiveDate};

/// The rate and cap a deployment is billed at, in cents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyRate {
    pub rate_minor: i64,
    pub cap_minor: i64,
}

/// The daily rate is the higher of the template's and the server type's; the
/// cap goes with the rate that wins (a template without its own cap is capped
/// at 30 days of its rate). A server type without a rate is refused: `None`.
pub fn resolve_daily_rate(
    template_rate: Option<f64>,
    template_cap: Option<f64>,
    server_type_rate: Option<(f64, f64)>,
) -> Option<DailyRate> {
    let (type_rate, type_cap) = server_type_rate?;
    let type_rate_minor = to_minor(type_rate);
    match template_rate.map(to_minor) {
        Some(template_rate_minor) if template_rate_minor > type_rate_minor => Some(DailyRate {
            rate_minor: template_rate_minor,
            cap_minor: template_cap
                .map(to_minor)
                .unwrap_or(template_rate_minor * 30),
        }),
        _ => Some(DailyRate {
            rate_minor: type_rate_minor,
            cap_minor: to_minor(type_cap),
        }),
    }
}

fn to_minor(amount: f64) -> i64 {
    (amount * 100.0).round() as i64
}

fn month_start(day: NaiveDate) -> NaiveDate {
    day.with_day(1).expect("day 1 exists in every month")
}

/// What is known about a deployment's reported usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageState {
    pub rate_minor: i64,
    pub cap_minor: i64,
    /// The day the deploy succeeded and usage started.
    pub started_on: NaiveDate,
    /// First day of the month `period_reported_minor` counts.
    pub period_start: Option<NaiveDate>,
    pub period_reported_minor: i64,
    pub last_reported_on: Option<NaiveDate>,
}

/// One day to report. `value_minor` is 0 once the month's cap is reached:
/// the day is marked as done, nothing is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayReport {
    pub day: NaiveDate,
    pub value_minor: i64,
    pub period_start: NaiveDate,
    pub period_reported_minor: i64,
}

/// The earliest day not yet reported, up to and including `until` (today for a
/// running server, the deletion day for a stopped one). Missed days are caught
/// up one by one; the monthly counter restarts on the 1st.
pub fn next_report(state: &UsageState, until: NaiveDate) -> Option<DayReport> {
    let day = match state.last_reported_on {
        Some(last) => last.succ_opt()?,
        None => state.started_on,
    };
    if day < state.started_on || day > until {
        return None;
    }
    let period_start = month_start(day);
    let reported = if state.period_start == Some(period_start) {
        state.period_reported_minor
    } else {
        0
    };
    let value_minor = state.rate_minor.min(state.cap_minor - reported).max(0);
    Some(DayReport {
        day,
        value_minor,
        period_start,
        period_reported_minor: reported + value_minor,
    })
}

/// The meter event identifier: one per deployment and day, so a resend of the
/// same day is not counted twice.
pub fn identifier(deployment_hash: &str, day: NaiveDate) -> String {
    format!("{}:{}", deployment_hash, day.format("%Y-%m-%d"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    const CPX32: (f64, f64) = (0.86, 26.00);
    const CPX42: (f64, f64) = (1.67, 50.00);

    #[test]
    fn the_rate_is_the_higher_of_the_template_and_the_server_type() {
        assert_eq!(
            resolve_daily_rate(None, None, Some(CPX32)),
            Some(DailyRate {
                rate_minor: 86,
                cap_minor: 2600
            })
        );
        assert_eq!(
            resolve_daily_rate(Some(0.50), Some(15.00), Some(CPX42)),
            Some(DailyRate {
                rate_minor: 167,
                cap_minor: 5000
            })
        );
        assert_eq!(
            resolve_daily_rate(Some(2.00), Some(55.00), Some(CPX32)),
            Some(DailyRate {
                rate_minor: 200,
                cap_minor: 5500
            })
        );
    }

    #[test]
    fn a_winning_template_rate_without_a_cap_is_capped_at_30_days() {
        assert_eq!(
            resolve_daily_rate(Some(2.00), None, Some(CPX32)),
            Some(DailyRate {
                rate_minor: 200,
                cap_minor: 6000
            })
        );
    }

    #[test]
    fn a_server_type_without_a_rate_is_refused() {
        assert_eq!(resolve_daily_rate(Some(2.00), Some(60.0), None), None);
        assert_eq!(resolve_daily_rate(None, None, None), None);
    }

    fn running(started_on: NaiveDate) -> UsageState {
        UsageState {
            rate_minor: 86,
            cap_minor: 2600,
            started_on,
            period_start: None,
            period_reported_minor: 0,
            last_reported_on: None,
        }
    }

    fn apply(state: &mut UsageState, report: &DayReport) {
        state.period_start = Some(report.period_start);
        state.period_reported_minor = report.period_reported_minor;
        state.last_reported_on = Some(report.day);
    }

    /// Drive the sweeper day by day; returns the value sent for each day.
    fn run(state: &mut UsageState, until: NaiveDate) -> Vec<(NaiveDate, i64)> {
        let mut sent = Vec::new();
        while let Some(report) = next_report(state, until) {
            apply(state, &report);
            sent.push((report.day, report.value_minor));
        }
        sent
    }

    #[test]
    fn the_first_day_is_the_day_usage_started() {
        let state = running(d(2026, 10, 16));
        let report = next_report(&state, d(2026, 10, 16)).unwrap();
        assert_eq!(report.day, d(2026, 10, 16));
        assert_eq!(report.value_minor, 86);
        assert_eq!(report.period_start, d(2026, 10, 1));
        assert_eq!(report.period_reported_minor, 86);
    }

    #[test]
    fn a_day_already_reported_is_not_reported_again() {
        let mut state = running(d(2026, 10, 16));
        run(&mut state, d(2026, 10, 16));
        assert_eq!(next_report(&state, d(2026, 10, 16)), None);
    }

    #[test]
    fn a_31_day_month_stops_at_the_cap() {
        let mut state = running(d(2026, 10, 1));
        let sent = run(&mut state, d(2026, 10, 31));
        assert_eq!(sent.len(), 31);
        assert!(sent[..30].iter().all(|(_, v)| *v == 86));
        assert_eq!(sent[30], (d(2026, 10, 31), 20));
        assert_eq!(sent.iter().map(|(_, v)| v).sum::<i64>(), 2600);
    }

    #[test]
    fn days_after_the_cap_are_marked_with_nothing_sent() {
        let mut state = UsageState {
            rate_minor: 167,
            cap_minor: 500,
            ..running(d(2026, 10, 1))
        };
        let sent = run(&mut state, d(2026, 10, 5));
        assert_eq!(
            sent.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
            vec![167, 167, 166, 0, 0]
        );
    }

    #[test]
    fn the_cap_starts_again_on_the_first() {
        let mut state = running(d(2026, 10, 1));
        run(&mut state, d(2026, 10, 31));
        let report = next_report(&state, d(2026, 11, 1)).unwrap();
        assert_eq!(report.day, d(2026, 11, 1));
        assert_eq!(report.value_minor, 86);
        assert_eq!(report.period_start, d(2026, 11, 1));
        assert_eq!(report.period_reported_minor, 86);
    }

    #[test]
    fn missed_days_are_caught_up_in_order() {
        let mut state = running(d(2026, 10, 16));
        run(&mut state, d(2026, 10, 16));
        let sent = run(&mut state, d(2026, 10, 19));
        assert_eq!(
            sent,
            vec![
                (d(2026, 10, 17), 86),
                (d(2026, 10, 18), 86),
                (d(2026, 10, 19), 86)
            ]
        );
    }

    #[test]
    fn a_server_deleted_on_the_10th_is_billed_up_to_the_10th() {
        let mut state = UsageState {
            rate_minor: 167,
            cap_minor: 5000,
            ..running(d(2026, 10, 1))
        };
        let sent = run(&mut state, d(2026, 10, 10));
        assert_eq!(sent.len(), 10);
        assert_eq!(sent.iter().map(|(_, v)| v).sum::<i64>(), 1670);
        assert_eq!(next_report(&state, d(2026, 10, 10)), None);
    }

    #[test]
    fn nothing_is_reported_before_usage_started() {
        let state = running(d(2026, 10, 16));
        assert_eq!(next_report(&state, d(2026, 10, 15)), None);
    }

    #[test]
    fn the_identifier_names_the_deployment_and_the_day() {
        assert_eq!(
            identifier("deployment_abc", d(2026, 10, 6)),
            "deployment_abc:2026-10-06"
        );
    }
}
