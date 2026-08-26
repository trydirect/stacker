//! `stacker monitor` — CLI-side container-health alarm + basic scheduler.
//!
//! Polls the deployment's live container health (via the Status Panel agent),
//! runs the pure [`health_monitor`] engine to edge-detect down/recovery
//! transitions, and fires the configured `monitoring.alerts.notify` target on a
//! change. State is persisted to `.stacker/monitor.state` so `--once` (cron)
//! invocations stay edge-triggered across runs.
//!
//! The alarm logic lives in the standalone `health-monitor` crate; this module
//! is just the I/O + scheduler shell around it.

use std::io::Write as _;
use std::path::PathBuf;

use health_monitor::{alert_message, detect_transition, parse_container_health, WatchState};

use crate::cli::config_parser::AlertTarget;
use crate::cli::error::CliError;
use crate::cli::runtime::CliRuntime;
use crate::console::commands::CallableTrait;

pub struct MonitorCommand {
    /// Run a single check and exit (cron-friendly). Otherwise loops forever.
    pub once: bool,
    /// Override the poll interval (seconds) from config.
    pub interval: Option<u64>,
    pub deployment: Option<String>,
}

impl MonitorCommand {
    pub fn new(once: bool, interval: Option<u64>, deployment: Option<String>) -> Self {
        Self {
            once,
            interval,
            deployment,
        }
    }
}

fn state_path() -> PathBuf {
    PathBuf::from(".stacker").join("monitor.state")
}

fn read_state() -> WatchState {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str(s.trim()).ok())
        .unwrap_or_default()
}

fn write_state(state: WatchState) {
    if let Some(parent) = state_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&state) {
        if let Ok(mut f) = std::fs::File::create(state_path()) {
            let _ = f.write_all(json.as_bytes());
        }
    }
}

/// Deliver an alert message via the configured `AlertTarget`.
/// - `Terminal` → OS/terminal notification (this is the "notify in terminal" target).
/// - `Webhook` → HTTP POST (ntfy/Slack/…).
/// - `Pipe` → acknowledged but deferred (needs the pipe runtime wired — follow-up).
fn dispatch(target: &AlertTarget, message: &str) -> Result<(), CliError> {
    match target {
        AlertTarget::Terminal { .. } => {
            crate::cli::notify::notify_message("Stacker container alert", message);
            Ok(())
        }
        AlertTarget::Webhook { url, method } => {
            let client = reqwest::blocking::Client::new();
            let method = reqwest::Method::from_bytes(method.to_ascii_uppercase().as_bytes())
                .unwrap_or(reqwest::Method::POST);
            let resp = client
                .request(method, url)
                .body(message.to_string())
                .send()
                .map_err(|e| CliError::ConfigValidation(format!("Alert delivery failed: {e}")))?;
            if !resp.status().is_success() {
                return Err(CliError::ConfigValidation(format!(
                    "Alert endpoint returned {}",
                    resp.status()
                )));
            }
            Ok(())
        }
        AlertTarget::Pipe { pipe } => {
            eprintln!(
                "  (alert would run pipe '{pipe}', but pipe-target alerts aren't wired yet — \
                 use a terminal or webhook target for now)"
            );
            Ok(())
        }
    }
}

impl CallableTrait for MonitorCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = std::env::current_dir().map_err(CliError::Io)?;
        let config = crate::cli::config_parser::StackerConfig::from_file(
            &project_dir.join("stacker.yml"),
        )
        .map_err(|e| CliError::ConfigValidation(format!("Failed to read stacker.yml: {e}")))?;

        let Some(alerts) = config.monitoring.alerts.clone() else {
            return Err(CliError::ConfigValidation(
                "No `monitoring.alerts` configured in stacker.yml. Add an `alerts:` block with a \
                 `notify:` target to enable the container-down alarm."
                    .into(),
            )
            .into());
        };
        let interval = self.interval.unwrap_or(alerts.interval).max(1);

        let ctx = CliRuntime::new("monitor")?;
        let hash = super::agent::resolve_deployment_hash(&self.deployment, &ctx)?;

        loop {
            // One check cycle.
            match super::agent::fetch_live_containers(&ctx, &hash) {
                Ok(containers) => {
                    let raw = serde_json::Value::Array(containers.unwrap_or_default());
                    let snapshot = parse_container_health(&raw);
                    let prev = read_state();
                    let transition = detect_transition(prev, &snapshot);

                    if let Some(message) = alert_message(&transition, alerts.on_recovery) {
                        println!("● {message}");
                        if let Err(e) = dispatch(&alerts.target, &message) {
                            eprintln!("  alert dispatch error: {e}");
                        }
                    } else {
                        println!(
                            "· {} container(s), all healthy",
                            snapshot.len()
                        );
                    }
                    // Persist the *current* health as the new baseline.
                    write_state(WatchState::from(health_monitor::evaluate(&snapshot)));
                }
                Err(e) => {
                    // A transient fetch failure shouldn't kill a long-running watch.
                    eprintln!("  health fetch failed: {e}");
                }
            }

            if self.once {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(interval));
        }
        Ok(())
    }
}
