//! # health-monitor
//!
//! Pure container-health **alarm engine** for TryDirect Stacker. It answers two
//! questions from a snapshot of container states:
//!
//! 1. Is the deployment healthy right now? (any container not `running` → down)
//! 2. Did we just **transition** down or recover, relative to the last snapshot?
//!    (edge-triggering, so a watcher notifies once per change — not every poll)
//!
//! It has **no I/O, no network, no scheduler**: the caller (the `stacker monitor`
//! CLI loop, or a future agent) fetches container health, feeds it in, and acts
//! on the returned [`Transition`]. State is a tiny serializable value the caller
//! persists between polls (e.g. `.stacker/monitor.state`) so edge-triggering
//! survives across one-shot `--once` invocations (cron-friendly).
//!
//! Input parsing is tolerant: [`parse_container_health`] reads the array shape
//! that `stacker agent health --json` emits (`[{name, status, ...}]`).

use serde::{Deserialize, Serialize};

/// A single container's health, reduced to what the alarm needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerHealth {
    pub name: String,
    /// Docker/agent status string, e.g. "running", "restarting", "exited".
    pub status: String,
}

impl ContainerHealth {
    /// A container is "up" only when it reports exactly `running`. Anything else
    /// (restarting, exited, paused, dead, created, …) counts as a problem.
    pub fn is_up(&self) -> bool {
        self.status.eq_ignore_ascii_case("running")
    }
}

/// Overall health of the set of containers at one point in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    /// Every container is running.
    Up,
    /// At least one container is not running.
    Down,
}

/// Persisted watcher state between polls. Serialize to `.stacker/monitor.state`.
/// `Unknown` is the correct initial value: the first poll establishes a baseline
/// and only *changes* alert thereafter (a fresh watcher against an already-down
/// stack fires immediately, which is what you want).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WatchState {
    #[default]
    Unknown,
    Up,
    Down,
}

impl From<Health> for WatchState {
    fn from(h: Health) -> Self {
        match h {
            Health::Up => WatchState::Up,
            Health::Down => WatchState::Down,
        }
    }
}

/// What changed between the previous [`WatchState`] and the current [`Health`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// No actionable change (still up, or still down).
    None,
    /// Crossed into a problem state — fire the alarm.
    WentDown { offenders: Vec<ContainerHealth> },
    /// Recovered to all-running — fire the (optional) recovery notice.
    Recovered,
}

/// Evaluate overall health from a snapshot.
pub fn evaluate(containers: &[ContainerHealth]) -> Health {
    if containers.iter().all(ContainerHealth::is_up) {
        Health::Up
    } else {
        Health::Down
    }
}

/// The containers that are currently not running (for the alert message).
pub fn offenders(containers: &[ContainerHealth]) -> Vec<ContainerHealth> {
    containers.iter().filter(|c| !c.is_up()).cloned().collect()
}

/// Edge-detect the transition from `prev` to the current snapshot. This is the
/// heart of the alarm: it returns [`Transition::WentDown`] / [`Transition::Recovered`]
/// only on a *change*, so the caller notifies once per event.
///
/// - `Unknown → Down` and `Up → Down` → `WentDown`
/// - `Unknown → Up` → `None` (baseline established silently)
/// - `Down → Up` → `Recovered`
/// - same-state → `None`
pub fn detect_transition(prev: WatchState, containers: &[ContainerHealth]) -> Transition {
    let now = evaluate(containers);
    match (prev, now) {
        (WatchState::Down, Health::Up) => Transition::Recovered,
        (WatchState::Up | WatchState::Unknown, Health::Down) => Transition::WentDown {
            offenders: offenders(containers),
        },
        // Unknown→Up (baseline), Up→Up, Down→Down: nothing to report.
        _ => Transition::None,
    }
}

/// A ready-to-send alert message for a [`Transition`], or `None` when there's
/// nothing to send (or a recovery when recovery notices are disabled).
pub fn alert_message(transition: &Transition, notify_on_recovery: bool) -> Option<String> {
    match transition {
        Transition::None => None,
        Transition::WentDown { offenders } => {
            let names: Vec<&str> = offenders.iter().map(|c| c.name.as_str()).collect();
            Some(format!(
                "⚠️ container problem: {} not running ({})",
                names.len(),
                if names.is_empty() {
                    "unknown".to_string()
                } else {
                    names.join(", ")
                }
            ))
        }
        Transition::Recovered if notify_on_recovery => {
            Some("✅ all containers recovered".to_string())
        }
        Transition::Recovered => None,
    }
}

/// Parse the JSON that `stacker agent health --json` emits: a top-level array of
/// objects with at least `name` and `status`. Unknown fields are ignored;
/// entries missing `name`/`status` are skipped rather than failing the whole
/// parse (a robust watcher shouldn't die on one odd row).
pub fn parse_container_health(json: &serde_json::Value) -> Vec<ContainerHealth> {
    let Some(arr) = json.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_string();
            let status = item.get("status")?.as_str()?.to_string();
            Some(ContainerHealth { name, status })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn c(name: &str, status: &str) -> ContainerHealth {
        ContainerHealth {
            name: name.into(),
            status: status.into(),
        }
    }

    #[test]
    fn evaluate_is_up_only_when_all_running() {
        assert_eq!(evaluate(&[c("a", "running"), c("b", "running")]), Health::Up);
        assert_eq!(evaluate(&[c("a", "running"), c("b", "restarting")]), Health::Down);
        assert_eq!(evaluate(&[]), Health::Up); // vacuously up
        // status match is case-insensitive
        assert_eq!(evaluate(&[c("a", "RUNNING")]), Health::Up);
    }

    #[test]
    fn offenders_lists_only_non_running() {
        let snap = [c("a", "running"), c("b", "exited"), c("c", "restarting")];
        let names: Vec<_> = offenders(&snap).into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["b", "c"]);
    }

    #[test]
    fn transition_edges_are_correct() {
        let up = [c("a", "running")];
        let down = [c("a", "restarting")];

        // baseline: Unknown→Up is silent; Unknown→Down fires
        assert_eq!(detect_transition(WatchState::Unknown, &up), Transition::None);
        assert!(matches!(
            detect_transition(WatchState::Unknown, &down),
            Transition::WentDown { .. }
        ));
        // Up→Down fires, Down→Up recovers
        assert!(matches!(
            detect_transition(WatchState::Up, &down),
            Transition::WentDown { .. }
        ));
        assert_eq!(detect_transition(WatchState::Down, &up), Transition::Recovered);
        // steady states are silent
        assert_eq!(detect_transition(WatchState::Up, &up), Transition::None);
        assert_eq!(detect_transition(WatchState::Down, &down), Transition::None);
    }

    #[test]
    fn alert_message_formats_and_respects_recovery_flag() {
        let down = Transition::WentDown {
            offenders: vec![c("project-app-1", "restarting")],
        };
        let msg = alert_message(&down, true).unwrap();
        assert!(msg.contains("project-app-1") && msg.contains("container problem"));

        assert_eq!(alert_message(&Transition::Recovered, true).as_deref(), Some("✅ all containers recovered"));
        assert_eq!(alert_message(&Transition::Recovered, false), None);
        assert_eq!(alert_message(&Transition::None, true), None);
    }

    #[test]
    fn parse_reads_agent_health_shape_and_skips_bad_rows() {
        let payload = json!([
            { "name": "project-app-1", "status": "running", "cpu_pct": 0.2 },
            { "name": "project-ntfy-1", "status": "restarting" },
            { "status": "running" },            // no name → skipped
            { "name": "x" }                      // no status → skipped
        ]);
        let parsed = parse_container_health(&payload);
        assert_eq!(parsed.len(), 2);
        assert_eq!(evaluate(&parsed), Health::Down);
        assert_eq!(offenders(&parsed)[0].name, "project-ntfy-1");
    }

    #[test]
    fn watch_state_round_trips_and_defaults_unknown() {
        assert_eq!(WatchState::default(), WatchState::Unknown);
        let s = serde_json::to_string(&WatchState::Down).unwrap();
        assert_eq!(s, "\"down\"");
        assert_eq!(WatchState::from(Health::Up), WatchState::Up);
    }
}
